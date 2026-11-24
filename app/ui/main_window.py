import webbrowser
from typing import Any, Optional
from PySide6.QtCore import QAbstractTableModel, QModelIndex, Qt, QThread, Signal
from PySide6.QtGui import QColor
from PySide6.QtWidgets import (
    QButtonGroup,
    QComboBox,
    QFileDialog,
    QFrame,
    QHBoxLayout,
    QHeaderView,
    QInputDialog,
    QLabel,
    QLineEdit,
    QMainWindow,
    QMessageBox,
    QProgressBar,
    QPushButton,
    QScrollArea,
    QTableView,
    QVBoxLayout,
    QWidget,
)
from app.bridge import bindings
from app.service.device_service import SourceOption, get_privilege, human_bytes, list_source_options
from app.service.export_service import export_selected
from app.service.session_store import SessionStore
from app.ui.widgets.status_badge import StatusPillDelegate


MAROON_BLACK_QSS = """
QMainWindow, QWidget {
    background-color: #0b090a;
    color: #f4f1f2;
    font-family: "Inter", "Ubuntu", "Segoe UI", system-ui, sans-serif;
    font-size: 13px;
}
QFrame#sidebar {
    background-color: #070607;
    border-right: 1px solid #241b1e;
}
QFrame#topbar {
    background-color: #0f0c0d;
    border-bottom: 1px solid #241b1e;
}
QFrame#bottombar {
    background-color: #0f0c0d;
    border-top: 1px solid #241b1e;
}
QLabel#brand {
    font-size: 18px;
    font-weight: 800;
    color: #f9f6f7;
    letter-spacing: 0.09em;
}
QLabel#brandSub {
    font-size: 10px;
    font-weight: 600;
    color: #be123c;
    letter-spacing: 0.14em;
}
QLabel#sectionTitle {
    font-size: 10px;
    font-weight: 700;
    color: #786b70;
    letter-spacing: 0.10em;
}
QPushButton#driveCard {
    background-color: #120e10;
    border: 1px solid #241b1e;
    border-radius: 10px;
    text-align: left;
    padding: 10px 12px;
    color: #d6cfd1;
}
QPushButton#driveCard:hover {
    background-color: #1c1417;
    border: 1px solid #4c1d2c;
}
QPushButton#driveCard:checked {
    background-color: #2b0d17;
    border: 1px solid #9f1239;
    color: #ffffff;
}
QPushButton#primary {
    background-color: qlineargradient(x1:0, y1:0, x2:1, y2:0, stop:0 #881337, stop:1 #9f1239);
    color: #ffffff;
    border: 1px solid #be123c;
    border-radius: 8px;
    padding: 8px 18px;
    font-weight: 700;
}
QPushButton#primary:hover {
    background-color: qlineargradient(x1:0, y1:0, x2:1, y2:0, stop:0 #9f1239, stop:1 #be123c);
}
QPushButton#primary:disabled {
    background-color: #1c1618;
    border-color: #2a2124;
    color: #6b5f63;
}
QPushButton#ghost {
    background-color: #141012;
    border: 1px solid #292023;
    border-radius: 8px;
    padding: 7px 12px;
    color: #d6cfd1;
    font-weight: 600;
}
QPushButton#ghost:hover {
    background-color: #21161a;
    border-color: #5c1f33;
    color: #ffffff;
}
QPushButton#ghost:disabled {
    color: #54494d;
    border-color: #1a1517;
}
QPushButton#pill {
    background-color: transparent;
    border: 1px solid #292023;
    border-radius: 14px;
    padding: 5px 13px;
    color: #a89ca0;
    font-size: 12px;
    font-weight: 600;
}
QPushButton#pill:hover {
    background-color: #1c1417;
    color: #f4f1f2;
}
QPushButton#pill:checked {
    background-color: #881337;
    border-color: #be123c;
    color: #ffffff;
}
QLineEdit, QComboBox {
    background-color: #141012;
    border: 1px solid #292023;
    border-radius: 8px;
    padding: 7px 12px;
    color: #f4f1f2;
}
QLineEdit:focus, QComboBox:focus {
    border: 1px solid #9f1239;
}
QComboBox::drop-down {
    border: none;
    width: 22px;
}
QComboBox QAbstractItemView {
    background-color: #141012;
    border: 1px solid #3a242b;
    selection-background-color: #881337;
    color: #f4f1f2;
}
QTableView {
    background-color: #110d0f;
    alternate-background-color: #151012;
    border: 1px solid #241b1e;
    border-radius: 12px;
    gridline-color: transparent;
    selection-background-color: #2a0e17;
    selection-color: #ffffff;
    outline: none;
}
QTableView::item {
    border-bottom: 1px solid #1e1619;
    padding: 4px 10px;
}
QHeaderView::section {
    background-color: #0d0a0b;
    color: #786b70;
    padding: 10px 10px;
    border: none;
    border-bottom: 1px solid #241b1e;
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
}
QProgressBar {
    background-color: #1f171a;
    border: none;
    border-radius: 3px;
    max-height: 5px;
}
QProgressBar::chunk {
    background-color: qlineargradient(x1:0, y1:0, x2:1, y2:0, stop:0 #881337, stop:1 #e11d48);
    border-radius: 3px;
}
QScrollBar:vertical {
    background: #0b090a;
    width: 7px;
    margin: 0;
}
QScrollBar::handle:vertical {
    background: #3b242b;
    border-radius: 3px;
    min-height: 24px;
}
QScrollBar::handle:vertical:hover {
    background: #881337;
}
QScrollBar::add-line:vertical, QScrollBar::sub-line:vertical {
    height: 0;
}
QScrollArea {
    border: none;
    background: transparent;
}
"""


class ScanWorker(QThread):
    finished_ok = Signal(dict)
    failed = Signal(str)

    def __init__(self, source_path: str, mode: str, limit: Optional[int]) -> None:
        super().__init__()
        self.source_path = source_path
        self.mode = mode
        self.limit = limit

    def run(self) -> None:
        try:
            res = bindings.scan_source(self.source_path, mode=self.mode, max_candidates=self.limit)
            self.finished_ok.emit(res)
        except Exception as exc:
            self.failed.emit(str(exc))


class ExportWorker(QThread):
    finished_ok = Signal(dict)
    failed = Signal(str)

    def __init__(
        self,
        source_path: str,
        fs_kind: str,
        mode: str,
        candidates: list[dict[str, Any]],
        dest_dir: str,
    ) -> None:
        super().__init__()
        self.source_path = source_path
        self.fs_kind = fs_kind
        self.mode = mode
        self.candidates = candidates
        self.dest_dir = dest_dir

    def run(self) -> None:
        try:
            res = export_selected(
                self.source_path, self.fs_kind, self.mode, self.candidates, self.dest_dir
            )
            self.finished_ok.emit(res)
        except Exception as exc:
            self.failed.emit(str(exc))


class CandidateTableModel(QAbstractTableModel):
    HEADERS = ["", "File", "Status", "Size", "Confidence"]

    def __init__(self) -> None:
        super().__init__()
        self._all: list[dict[str, Any]] = []
        self._filtered: list[dict[str, Any]] = []
        self._selected_ids: set[str] = set()
        self._query = ""
        self._status_filter = "All"

    def set_candidates(self, items: list[dict[str, Any]]) -> None:
        self.beginResetModel()
        self._all = items
        self._selected_ids.clear()
        self._apply_filter_locked()
        self.endResetModel()

    def set_filter(self, query: str, status: str) -> None:
        self.beginResetModel()
        self._query = query.strip().lower()
        self._status_filter = status
        self._apply_filter_locked()
        self.endResetModel()

    def select_all_intact(self) -> None:
        self.beginResetModel()
        for c in self._filtered:
            if c.get("status") == "Intact":
                self._selected_ids.add(c["id_hex"])
        self.endResetModel()

    def _apply_filter_locked(self) -> None:
        out = []
        for c in self._all:
            if self._status_filter != "All" and c.get("status") != self._status_filter:
                continue
            if self._query and self._query not in c.get("path_display", "").lower():
                continue
            out.append(c)
        self._filtered = out

    def selected_candidates(self) -> list[dict[str, Any]]:
        return [c for c in self._all if c["id_hex"] in self._selected_ids]

    def selected_count(self) -> int:
        return len(self._selected_ids)

    def stats(self) -> dict[str, int]:
        counts = {"Total": len(self._all), "Intact": 0, "Partial": 0, "Unrecoverable": 0}
        for c in self._all:
            st = str(c.get("status", ""))
            if st in counts:
                counts[st] += 1
        return counts

    def rowCount(self, parent: QModelIndex = QModelIndex()) -> int:
        return 0 if parent.isValid() else len(self._filtered)

    def columnCount(self, parent: QModelIndex = QModelIndex()) -> int:
        return 0 if parent.isValid() else len(self.HEADERS)

    def headerData(self, section: int, orientation: Qt.Orientation, role: int = Qt.DisplayRole) -> Any:
        if role == Qt.DisplayRole and orientation == Qt.Horizontal:
            return self.HEADERS[section]
        return None

    def flags(self, index: QModelIndex) -> Qt.ItemFlags:
        base = Qt.ItemIsEnabled | Qt.ItemIsSelectable
        if index.column() == 0:
            return base | Qt.ItemIsUserCheckable
        return base

    def data(self, index: QModelIndex, role: int = Qt.DisplayRole) -> Any:
        if not index.isValid():
            return None
        c = self._filtered[index.row()]
        col = index.column()
        if col == 0 and role == Qt.CheckStateRole:
            return Qt.Checked if c["id_hex"] in self._selected_ids else Qt.Unchecked
        if role == Qt.DisplayRole:
            if col == 1:
                return c.get("path_display", "")
            if col == 2:
                return c.get("status", "")
            if col == 3:
                return human_bytes(int(c.get("logical_size", 0)))
            if col == 4:
                return f"{c.get('confidence', 0)}%"
        if role == Qt.ForegroundRole and col in (3, 4):
            return QColor("#a89ca0")
        return None

    def setData(self, index: QModelIndex, value: Any, role: int = Qt.EditRole) -> bool:
        if index.column() == 0 and role == Qt.CheckStateRole:
            cid = self._filtered[index.row()]["id_hex"]
            if value == Qt.Checked or value == Qt.CheckState.Checked.value:
                self._selected_ids.add(cid)
            else:
                self._selected_ids.discard(cid)
            self.dataChanged.emit(index, index, [Qt.CheckStateRole])
            return True
        return False


class MainWindow(QMainWindow):
    def __init__(self) -> None:
        super().__init__()
        self.setWindowTitle("RECOVER-X — Data Recovery")
        self.resize(1160, 740)
        self.setStyleSheet(MAROON_BLACK_QSS)

        self.store = SessionStore()
        self.current_source = ""
        self.current_fs_kind = "Unknown"
        self.current_mode = "quick"
        self.last_report_html: Optional[str] = None
        self.scan_worker: Optional[ScanWorker] = None
        self.export_worker: Optional[ExportWorker] = None

        root = QWidget()
        root_lay = QHBoxLayout(root)
        root_lay.setContentsMargins(0, 0, 0, 0)
        root_lay.setSpacing(0)
        self.setCentralWidget(root)

        # ==================== SLIM SIDEBAR ====================
        sidebar = QFrame()
        sidebar.setObjectName("sidebar")
        sidebar.setFixedWidth(240)
        sb_lay = QVBoxLayout(sidebar)
        sb_lay.setContentsMargins(16, 22, 16, 16)
        sb_lay.setSpacing(12)

        brand = QLabel("RECOVER-X")
        brand.setObjectName("brand")
        sub = QLabel("DATA RECOVERY")
        sub.setObjectName("brandSub")
        sb_lay.addWidget(brand)
        sb_lay.addWidget(sub)
        sb_lay.addSpacing(6)

        drv_hdr = QHBoxLayout()
        t_drv = QLabel("DRIVES")
        t_drv.setObjectName("sectionTitle")
        drv_hdr.addWidget(t_drv)
        btn_ref = QPushButton("Refresh")
        btn_ref.setCursor(Qt.PointingHandCursor)
        btn_ref.setStyleSheet("QPushButton{background:transparent;border:none;color:#a89ca0;font-size:11px;padding:2px 4px;} QPushButton:hover{color:#fda4af;}")
        btn_ref.clicked.connect(lambda: self.refresh_sources(notify=True))
        drv_hdr.addWidget(btn_ref)
        sb_lay.addLayout(drv_hdr)

        self.drive_scroll = QScrollArea()
        self.drive_scroll.setWidgetResizable(True)
        self.drive_container = QWidget()
        self.drive_list_lay = QVBoxLayout(self.drive_container)
        self.drive_list_lay.setContentsMargins(0, 0, 4, 0)
        self.drive_list_lay.setSpacing(8)
        self.drive_list_lay.addStretch(1)
        self.drive_scroll.setWidget(self.drive_container)
        self.drive_group = QButtonGroup(self)
        self.drive_group.setExclusive(True)
        sb_lay.addWidget(self.drive_scroll, stretch=1)

        btn_img = QPushButton("Open Disk Image")
        btn_img.setObjectName("ghost")
        btn_img.clicked.connect(self.pick_image)
        sb_lay.addWidget(btn_img)

        btn_res = QPushButton("Saved Sessions")
        btn_res.setObjectName("ghost")
        btn_res.clicked.connect(self.resume_session)
        sb_lay.addWidget(btn_res)

        priv = get_privilege()
        priv_lbl = QLabel("● Read-Only Ready" if priv.is_elevated else "● Run with sudo for drives")
        priv_lbl.setStyleSheet(
            "color:#34d399; font-size:11px; padding-top:4px;"
            if priv.is_elevated
            else "color:#fbbf24; font-size:11px; padding-top:4px;"
        )
        sb_lay.addWidget(priv_lbl)

        root_lay.addWidget(sidebar)

        # ==================== MAIN AREA ====================
        main = QWidget()
        main_lay = QVBoxLayout(main)
        main_lay.setContentsMargins(0, 0, 0, 0)
        main_lay.setSpacing(0)

        # Clean Top Bar
        topbar = QFrame()
        topbar.setObjectName("topbar")
        tb_lay = QHBoxLayout(topbar)
        tb_lay.setContentsMargins(24, 16, 24, 16)
        tb_lay.setSpacing(12)

        title_col = QVBoxLayout()
        title_col.setSpacing(2)
        self.selected_title = QLabel("Choose a drive to scan")
        self.selected_title.setStyleSheet("font-size: 16px; font-weight: 700; color: #f9f6f7;")
        self.selected_sub = QLabel("Read-only mode · Your drive will not be modified")
        self.selected_sub.setStyleSheet("font-size: 12px; color: #a89ca0;")
        title_col.addWidget(self.selected_title)
        title_col.addWidget(self.selected_sub)
        tb_lay.addLayout(title_col, stretch=1)

        self.mode_combo = QComboBox()
        self.mode_combo.addItems(["Quick Scan", "Deep Scan"])
        self.mode_combo.setMinimumWidth(120)
        tb_lay.addWidget(self.mode_combo)

        self.limit_combo = QComboBox()
        self.limit_combo.addItems(["500 files", "2000 files", "All files"])
        self.limit_combo.setMinimumWidth(100)
        tb_lay.addWidget(self.limit_combo)

        self.btn_scan = QPushButton("Scan")
        self.btn_scan.setObjectName("primary")
        self.btn_scan.clicked.connect(self.start_scan)
        tb_lay.addWidget(self.btn_scan)
        main_lay.addWidget(topbar)

        # Workspace Body
        body = QWidget()
        body_lay = QVBoxLayout(body)
        body_lay.setContentsMargins(24, 18, 24, 16)
        body_lay.setSpacing(14)

        # Search + Compact Filter Pills
        filt_row = QHBoxLayout()
        filt_row.setSpacing(8)
        self.search_input = QLineEdit()
        self.search_input.setPlaceholderText("Search files...")
        self.search_input.textChanged.connect(self.apply_filters)
        filt_row.addWidget(self.search_input, stretch=1)

        self.pill_group = QButtonGroup(self)
        self.pill_group.setExclusive(True)
        self._active_status = "All"
        for st in ("All", "Intact", "Partial", "Unrecoverable"):
            p = QPushButton(st)
            p.setObjectName("pill")
            p.setCheckable(True)
            if st == "All":
                p.setChecked(True)
            p.clicked.connect(lambda checked, s=st: self._set_status_pill(s))
            self.pill_group.addButton(p)
            filt_row.addWidget(p)

        btn_sel = QPushButton("Select Intact")
        btn_sel.setObjectName("ghost")
        btn_sel.clicked.connect(self._select_all_intact)
        filt_row.addWidget(btn_sel)
        body_lay.addLayout(filt_row)

        # Clean File Table
        self.model = CandidateTableModel()
        self.model.dataChanged.connect(self.update_selection_label)
        self.table = QTableView()
        self.table.setModel(self.model)
        self.table.setAlternatingRowColors(True)
        self.table.setShowGrid(False)
        self.table.verticalHeader().setVisible(False)
        self.table.verticalHeader().setDefaultSectionSize(40)
        self.table.setSelectionBehavior(QTableView.SelectRows)
        self.table.setSelectionMode(QTableView.SingleSelection)
        self.table.setItemDelegateForColumn(2, StatusPillDelegate(self.table))
        self.table.horizontalHeader().setSectionResizeMode(1, QHeaderView.Stretch)
        self.table.setColumnWidth(0, 40)
        self.table.setColumnWidth(2, 126)
        self.table.setColumnWidth(3, 96)
        self.table.setColumnWidth(4, 92)
        body_lay.addWidget(self.table, stretch=1)
        main_lay.addWidget(body, stretch=1)

        # Sleek full-width 3px progress line (hidden when idle)
        self.progress = QProgressBar()
        self.progress.setTextVisible(False)
        self.progress.setFixedHeight(3)
        self.progress.setRange(0, 0)
        self.progress.setVisible(False)
        main_lay.addWidget(self.progress)

        # Clean Bottom Bar
        botbar = QFrame()
        botbar.setObjectName("bottombar")
        bot_lay = QHBoxLayout(botbar)
        bot_lay.setContentsMargins(24, 12, 24, 12)
        bot_lay.setSpacing(12)

        self.status_label = QLabel("Ready")
        self.status_label.setStyleSheet("color: #a89ca0; font-size: 12px;")
        bot_lay.addWidget(self.status_label, stretch=1)

        self.btn_report = QPushButton("View Report")
        self.btn_report.setObjectName("ghost")
        self.btn_report.setEnabled(False)
        self.btn_report.clicked.connect(self.open_report)
        bot_lay.addWidget(self.btn_report)

        self.btn_export = QPushButton("Recover Selected")
        self.btn_export.setObjectName("primary")
        self.btn_export.setEnabled(False)
        self.btn_export.clicked.connect(self.start_export)
        bot_lay.addWidget(self.btn_export)
        main_lay.addWidget(botbar)

        root_lay.addWidget(main, stretch=1)
        self.refresh_sources()

    def refresh_sources(self, notify: bool = False) -> None:
        while self.drive_list_lay.count() > 1:
            item = self.drive_list_lay.takeAt(0)
            if item.widget():
                item.widget().deleteLater()

        options = list_source_options()
        for idx, opt in enumerate(options):
            mp = opt.mount_point.split("/")[-1] if opt.mount_point and opt.mount_point != "/" else (opt.mount_point or "Unmounted")
            kind_tag = "HDD" if opt.media_kind == "Hdd" else f"{opt.media_kind.upper()} · TRIM"
            btn = QPushButton(f"{mp}  ({human_bytes(opt.size_bytes)})\n{opt.os_path} · {kind_tag}")
            btn.setObjectName("driveCard")
            btn.setCheckable(True)
            btn.clicked.connect(lambda checked, o=opt: self._select_source_option(o))
            self.drive_group.addButton(btn)
            self.drive_list_lay.insertWidget(self.drive_list_lay.count() - 1, btn)
            if idx == 0 and not self.current_source:
                btn.setChecked(True)
                self._select_source_option(opt)
        if notify:
            self.status_label.setText(f'Refreshed · {len(options)} drive partition(s) detected')

    def _select_source_option(self, opt: SourceOption) -> None:
        self.current_source = opt.os_path
        label = opt.mount_point or opt.os_path
        self.selected_title.setText(f"{label}  ({human_bytes(opt.size_bytes)})")
        if opt.media_kind == "Ssd" and opt.trim_status == "Enabled":
            self.selected_sub.setText(f"{opt.os_path} · {opt.device_model} · SSD TRIM enabled (deleted data may be zeroed)")
            self.selected_sub.setStyleSheet("font-size: 12px; color: #fbbf24;")
        else:
            self.selected_sub.setText(f"{opt.os_path} · {opt.device_model} · {opt.media_kind} · Read-only")
            self.selected_sub.setStyleSheet("font-size: 12px; color: #a89ca0;")

    def pick_image(self) -> None:
        path, _ = QFileDialog.getOpenFileName(
            self, "Open Disk Image", "", "Disk Images (*.dd *.raw *.img);;All Files (*)",
            options=QFileDialog.DontUseNativeDialog,
        )
        if path:
            self.current_source = path
            self.selected_title.setText(path.split("/")[-1])
            self.selected_sub.setText(f"Disk Image · {path}")
            self.selected_sub.setStyleSheet("font-size: 12px; color: #fda4af;")

    def resume_session(self) -> None:
        sessions = self.store.list_sessions(20)
        if not sessions:
            QMessageBox.information(self, "No Saved Sessions", "Run a scan first to save a session.")
            return
        labels = [
            f"#{s['id']} · {s['source_path']} ({s['candidate_count']} files) · {s['created_utc'][:16]}"
            for s in sessions
        ]
        choice, ok = QInputDialog.getItem(self, "Saved Sessions", "Choose a session:", labels, 0, False)
        if not ok:
            return
        sid = sessions[labels.index(choice)]["id"]
        loaded = self.store.load_session(sid)
        if loaded:
            self.current_source = loaded["source_path"]
            self.current_fs_kind = loaded["fs_kind"]
            self.current_mode = loaded["mode"]
            self.selected_title.setText(f"{self.current_source} ({self.current_fs_kind})")
            self.model.set_candidates(loaded["candidates"])
            self._update_summary()
            self.update_selection_label()

    def _set_status_pill(self, status: str) -> None:
        self._active_status = status
        self.apply_filters()

    def apply_filters(self) -> None:
        self.model.set_filter(self.search_input.text(), self._active_status)

    def _select_all_intact(self) -> None:
        self.model.select_all_intact()
        self.update_selection_label()

    def _update_summary(self) -> None:
        st = self.model.stats()
        self.status_label.setText(
            f"{st['Total']} found  ·  {st['Intact']} intact  ·  {st['Partial']} partial  ·  {st['Unrecoverable']} unrecoverable"
        )

    def update_selection_label(self) -> None:
        cnt = self.model.selected_count()
        self.btn_export.setEnabled(cnt > 0 and bool(self.current_source))
        self.btn_export.setText(f"Recover Selected ({cnt})" if cnt > 0 else "Recover Selected")

    def start_scan(self) -> None:
        if not self.current_source:
            QMessageBox.warning(self, "No Drive Selected", "Select a drive on the left first.")
            return
        mode = "deep" if self.mode_combo.currentIndex() == 1 else "quick"
        lim_text = self.limit_combo.currentText()
        limit = 500 if "500" in lim_text else (2000 if "2000" in lim_text else None)

        self.current_mode = mode
        self.btn_scan.setEnabled(False)
        self.progress.setVisible(True)
        self.status_label.setText(f"Scanning {self.current_source}...")

        self.scan_worker = ScanWorker(self.current_source, mode, limit)
        self.scan_worker.finished_ok.connect(self.on_scan_done)
        self.scan_worker.failed.connect(self.on_scan_failed)
        self.scan_worker.start()

    def on_scan_done(self, payload: dict[str, Any]) -> None:
        self.progress.setVisible(False)
        self.btn_scan.setEnabled(True)
        self.current_fs_kind = str(payload.get("fs_kind", "Unknown"))
        cands = payload.get("candidates", [])
        self.model.set_candidates(cands)
        self.store.save_session(self.current_source, self.current_fs_kind, self.current_mode, cands)
        self._update_summary()
        self.update_selection_label()

    def on_scan_failed(self, err: str) -> None:
        self.progress.setVisible(False)
        self.btn_scan.setEnabled(True)
        self.status_label.setText(f"Error: {err}")
        QMessageBox.critical(self, "Scan Error", err)

    def start_export(self) -> None:
        selected = self.model.selected_candidates()
        if not selected:
            return
        dest = QFileDialog.getExistingDirectory(
            self, "Choose Recovery Folder (Different Drive)", "",
            options=QFileDialog.DontUseNativeDialog,
        )
        if not dest:
            return
        self.btn_export.setEnabled(False)
        self.progress.setVisible(True)
        self.status_label.setText(f"Recovering {len(selected)} file(s)...")

        self.export_worker = ExportWorker(
            self.current_source, self.current_fs_kind, self.current_mode, selected, dest
        )
        self.export_worker.finished_ok.connect(self.on_export_done)
        self.export_worker.failed.connect(self.on_export_failed)
        self.export_worker.start()

    def on_export_done(self, res: dict[str, Any]) -> None:
        self.progress.setVisible(False)
        self.update_selection_label()
        self.last_report_html = res["html_path"]
        self.btn_report.setEnabled(True)
        self.status_label.setText(f"Recovered {len(res['records'])} file(s)")
        QMessageBox.information(
            self,
            "Recovery Complete",
            f"Recovered {len(res['records'])} file(s) with SHA-256 verification.\n\nReport saved to:\n{res['html_path']}",
        )

    def on_export_failed(self, err: str) -> None:
        self.progress.setVisible(False)
        self.update_selection_label()
        self.status_label.setText(f"Export blocked: {err}")
        QMessageBox.warning(self, "Recovery Blocked", err)

    def open_report(self) -> None:
        if self.last_report_html:
            webbrowser.open(f"file://{self.last_report_html}")
