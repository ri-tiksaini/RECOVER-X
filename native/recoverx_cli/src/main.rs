use recoverx_core::core::cancel::CancelToken;
use recoverx_core::export::extractor::export_candidates;
use recoverx_core::fs::{scan_volume, ScanMode};
use recoverx_core::platform::{device, image_reader, partition, privilege};
use recoverx_core::platform::raw_reader::RawReader;
use recoverx_core::platform::trim::MediaKind;
use std::env;
use std::path::Path;
use std::sync::Arc;

fn open_reader(target: &str) -> Arc<dyn RawReader> {
    let path = Path::new(target);
    if path.is_file() {
        Arc::new(image_reader::ImageReader::open(path, 512).expect("open image failed"))
    } else {
        #[cfg(unix)]
        {
            Arc::new(
                recoverx_core::platform::raw_reader_linux::LinuxRawReader::open(path)
                    .expect("open device failed (try sudo)"),
            )
        }
        #[cfg(not(unix))]
        {
            Arc::new(
                recoverx_core::platform::raw_reader_windows::WindowsRawReader::open(path)
                    .expect("open device failed"),
            )
        }
    }
}

fn parse_mode(args: &[String]) -> ScanMode {
    if args.windows(2).any(|w| w[0] == "--mode" && w[1].eq_ignore_ascii_case("deep")) {
        ScanMode::Deep
    } else {
        ScanMode::Quick
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage:");
        eprintln!("  recoverx-cli devices");
        eprintln!("  recoverx-cli inspect <device_or_image>");
        eprintln!("  recoverx-cli scan <partition_or_image> [--mode quick|deep] [--limit N]");
        eprintln!("  recoverx-cli export <partition_or_image> <candidate_id_hex> <dest_dir> [--mode quick|deep]");
        std::process::exit(2);
    }

    match args[1].as_str() {
        "privilege" => {
            println!("{}", serde_json::to_string_pretty(&privilege::check_privilege()).unwrap());
        }
        "devices" => match device::enumerate_devices() {
            Ok(list) => println!("{}", serde_json::to_string_pretty(&list).unwrap()),
            Err(e) => {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        },
        "inspect" => {
            let target = args.get(2).expect("missing path");
            let reader = open_reader(target);
            let parts = partition::parse_partitions(reader.as_ref()).unwrap_or_default();
            let fs_kind = recoverx_core::fs::detect_fs(reader.as_ref()).ok();
            let backup = partition::probe_backup_fs_hint(reader.as_ref());
            let out = serde_json::json!({
                "path": target,
                "size_bytes": reader.size(),
                "sector_size": reader.sector_size(),
                "fs_kind": fs_kind,
                "partitions": parts,
                "backup_fs_hint": backup,
            });
            println!("{}", serde_json::to_string_pretty(&out).unwrap());
        }
        "scan" => {
            let target = args.get(2).expect("missing partition/image path");
            let mode = parse_mode(&args);
            let limit = args
                .windows(2)
                .find(|w| w[0] == "--limit")
                .and_then(|w| w[1].parse::<usize>().ok());
            let reader = open_reader(target);
            match scan_volume(reader, target.clone(), MediaKind::Hdd, mode, &CancelToken::new(), limit) {
                Ok((fs_kind, cands)) => {
                    let summary = serde_json::json!({
                        "source": target,
                        "mode": format!("{mode:?}"),
                        "fs_kind": fs_kind,
                        "deleted_found": cands.len(),
                        "candidates": cands.iter().map(|c| serde_json::json!({
                            "id": c.id_hex,
                            "path": c.path_display,
                            "size": c.logical_size,
                            "origin": c.origin,
                            "status": c.status,
                            "reason": c.reason_code,
                            "confidence": c.confidence,
                        })).collect::<Vec<_>>()
                    });
                    println!("{}", serde_json::to_string_pretty(&summary).unwrap());
                }
                Err(e) => {
                    eprintln!("Scan error: {e}");
                    std::process::exit(1);
                }
            }
        }
        "export" => {
            let target = args.get(2).expect("missing source");
            let id_hex = args.get(3).expect("missing candidate_id_hex");
            let dest = args.get(4).expect("missing dest_dir");
            let mode = parse_mode(&args);
            let reader = open_reader(target);
            let (_fs, cands) = scan_volume(
                Arc::clone(&reader),
                target.clone(),
                MediaKind::Hdd,
                mode,
                &CancelToken::new(),
                None,
            )
            .expect("scan failed");
            let chosen: Vec<_> = cands.into_iter().filter(|c| c.id_hex == *id_hex).collect();
            if chosen.is_empty() {
                eprintln!("Candidate {id_hex} not found.");
                std::process::exit(1);
            }
            match export_candidates(Path::new(target), reader.as_ref(), &chosen, Path::new(dest), &CancelToken::new()) {
                Ok(recs) => println!("{}", serde_json::to_string_pretty(&recs).unwrap()),
                Err(e) => {
                    eprintln!("Export refused/failed: {e}");
                    std::process::exit(1);
                }
            }
        }
        other => {
            eprintln!("Unknown command: {other}");
            std::process::exit(2);
        }
    }
}
