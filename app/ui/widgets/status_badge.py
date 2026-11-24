from PySide6.QtCore import QRect, Qt
from PySide6.QtGui import QColor, QFont, QPainter, QPen
from PySide6.QtWidgets import QStyle, QStyledItemDelegate, QStyleOptionViewItem


BADGE_STYLES = {
    "Intact": (QColor("#0a2e23"), QColor("#34d399"), QColor("#115e46")),
    "Partial": (QColor("#331c08"), QColor("#fbbf24"), QColor("#6b3b10")),
    "Unrecoverable": (QColor("#3b0d1b"), QColor("#fda4af"), QColor("#7f1d38")),
    "LowConfidence": (QColor("#211c1e"), QColor("#a89ca0"), QColor("#3a3034")),
}


def status_color(status: str) -> QColor:
    return BADGE_STYLES.get(status, (QColor("#211c1e"), QColor("#f4f1f2"), QColor("#3a3034")))[1]


class StatusPillDelegate(QStyledItemDelegate):
    """Minimal rounded pill badge for recovery status."""

    def paint(self, painter: QPainter, option: QStyleOptionViewItem, index) -> None:
        painter.save()
        painter.setRenderHint(QPainter.Antialiasing, True)

        if option.state & QStyle.State_Selected:
            painter.fillRect(option.rect, QColor("#2a0e17"))

        text = str(index.data(Qt.DisplayRole) or "")
        bg, fg, border = BADGE_STYLES.get(
            text, (QColor("#211c1e"), QColor("#d6cfd1"), QColor("#3a3034"))
        )

        pill_h = 22
        pill_w = min(option.rect.width() - 12, 106)
        x = option.rect.x() + 6
        y = option.rect.y() + (option.rect.height() - pill_h) // 2
        rect = QRect(x, y, pill_w, pill_h)

        painter.setBrush(bg)
        painter.setPen(QPen(border, 1))
        painter.drawRoundedRect(rect, 11, 11)

        font = QFont(option.font)
        font.setPointSize(9)
        font.setWeight(QFont.DemiBold)
        painter.setFont(font)
        painter.setPen(fg)
        painter.drawText(rect, Qt.AlignCenter, text)
        painter.restore()
