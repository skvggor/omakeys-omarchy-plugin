import QtQuick
import Quickshell
import Quickshell.Hyprland
import Quickshell.Wayland
import qs.Commons
import qs.Ui

Item {
    id: root

    property var service: null
    property bool tick: false
    readonly property int lingerMs: 700
    readonly property int historyWindowMs: 1400
    readonly property int maxHistoryRows: 4
    readonly property string focusedName: Hyprland.focusedMonitor ? String(Hyprland.focusedMonitor.name || "") : ""

    readonly property string comboText: root.service ? root.service.comboText : ""

    function textWidth(text, pixelSize, letterSpacing) {
        var length = String(text || "").length
        return Math.ceil(length * (pixelSize * 0.63 + letterSpacing)) + 6
    }

    function maskedText(text) {
        if (root.service && root.service.hideCharacterKeys) {
            return text.replace(/./g, '\u25CF')
        }
        return text
    }

    readonly property string lastEventText: {
        var events = root.service && root.service.state && root.service.state.events
        if (!events || events.length === 0) return ""
        var entry = events[0]
        return entry && entry.text ? String(entry.text) : ""
    }

    readonly property bool lastEventFresh: {
        void root.tick
        var events = root.service && root.service.state && root.service.state.events
        if (!events || events.length === 0) return false
        var at = Number(events[0].at || 0)
        return at > 0 && Date.now() - at <= root.lingerMs
    }

    readonly property string currentText: {
        void root.tick
        if (root.comboText !== "") return root.comboText
        if (root.lastEventFresh) return root.lastEventText
        return ""
    }

    property string displayedText: ""

    onCurrentTextChanged: {
        if (root.currentText !== "") root.displayedText = root.currentText
    }

    readonly property bool activityFresh: {
        void root.tick
        var state = root.service ? root.service.state : null
        if (!state || !state.activeAt) return false
        return Date.now() - Number(state.activeAt) <= root.historyWindowMs
    }

    readonly property var rows: {
        var out = []
        if (root.displayedText !== "") out.push({ text: root.displayedText, rank: 0 })
        var chips = root.service ? root.service.historyChips : []
        var start = 0
        if (chips.length > 0 && String(chips[0].text || "") === root.displayedText) start = 1
        var count = 0
        for (var index = start; index < chips.length && count < root.maxHistoryRows; index++) {
            var text = String(chips[index].text || "")
            if (text === "") continue
            out.push({ text: text, rank: Math.min(1 + count, 2) })
            count++
        }
        return out
    }

    function rankOf(index) {
        return index === 0 ? 0 : Math.min(index, 2)
    }

    function rankPixelSize(rank) {
        return rank === 0 ? Style.font.display : rank === 1 ? Style.font.title : Style.font.caption
    }

    function rowHeight(rank) {
        return rank === 0 ? Style.space(30) : rank === 1 ? Style.space(19) : Style.space(14)
    }

    function rowY(index) {
        var y = 0
        for (var i = 0; i < index; i++) {
            y += rowHeight(root.rankOf(i)) + Style.space(6)
        }
        return Math.round(y)
    }

    function contentWidth() {
        var widest = 0
        for (var i = 0; i < root.rows.length; i++) {
            var rank = root.rankOf(i)
            var prefix = rank >= 1 ? Style.space(20) : 0
            var width = prefix + textWidth(root.rows[i].text, rankPixelSize(rank), rank === 0 ? 1.4 : 1.0)
            if (width > widest) widest = width
        }
        return Math.min(widest, 600)
    }

    function contentHeight() {
        if (root.rows.length === 0) return 0
        return Math.round(root.rowY(root.rows.length - 1) + rowHeight(root.rankOf(root.rows.length - 1)))
    }

    readonly property bool visibleNow: {
        void root.tick
        if (!root.service) return false
        if (!root.service.enabled) return false
        if (!root.service.overlayEnabled) return false
        return root.rows.length > 0 && root.activityFresh
    }

    Timer {
        interval: 80
        repeat: true
        running: true
        onTriggered: root.tick = !root.tick
    }

    Variants {
        model: Quickshell.screens

        delegate: Component {
            PanelWindow {
                id: overlayWindow
                required property var modelData
                readonly property int pad: Style.space(16)
                property real fallOffset: modelData.height * 0.4

                screen: modelData
                visible: (root.visibleNow || card.slideOffset !== overlayWindow.fallOffset)
                    && String(modelData.name || "") === root.focusedName
                color: "transparent"
                anchors { top: true; bottom: true; left: true; right: true }
                WlrLayershell.namespace: "omarchy-omakeys"
                WlrLayershell.layer: WlrLayer.Overlay
                WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
                exclusionMode: ExclusionMode.Ignore
                mask: Region {}

                Connections {
                    target: root
                    enabled: String(modelData.name || "") === root.focusedName
                    function onVisibleNowChanged() {
                        if (root.visibleNow) {
                            fallAnimation.stop()
                            riseAnimation.from = overlayWindow.fallOffset
                            riseAnimation.start()
                        } else {
                            riseAnimation.stop()
                            fallAnimation.start()
                        }
                    }
                }

                NumberAnimation {
                    id: riseAnimation
                    target: card
                    property: "slideOffset"
                    to: 0
                    duration: 360
                    easing.type: Easing.OutQuart
                }

                NumberAnimation {
                    id: fallAnimation
                    target: card
                    property: "slideOffset"
                    to: overlayWindow.fallOffset
                    duration: 400
                    easing.type: Easing.InQuart
                    onFinished: root.displayedText = ""
                }

                Rectangle {
                    id: glow
                    width: card.width + Style.space(14)
                    height: card.height + Style.space(14)
                    anchors.horizontalCenter: parent.horizontalCenter
                    anchors.verticalCenter: parent.verticalCenter
                    anchors.verticalCenterOffset: card.anchors.verticalCenterOffset
                    radius: Style.cornerRadius + Style.space(7)
                    color: "transparent"
                    border.width: Style.space(7)
                    border.color: Util.alpha(Color.accent, 0.09)
                    opacity: card.opacity
                    scale: card.scale
                }

                BorderSurface {
                    id: card
                    property real slideOffset: 0
                    Component.onCompleted: slideOffset = overlayWindow.fallOffset
                    anchors.horizontalCenter: parent.horizontalCenter
                    anchors.verticalCenter: parent.verticalCenter
                    anchors.verticalCenterOffset: Math.round(parent.height * ((root.service.overlayVerticalPercent - 50) / 100)) + slideOffset
                    color: Util.alpha(Color.background, 0.78)
                    borderSpec: Border.surfaceSpec("popups", "border", Color.popups.border, Math.max(1, Style.space(2)))
                    radius: Style.cornerRadius
                    width: card.borderLeft + pad + root.contentWidth() + pad + card.borderRight
                    height: card.borderTop + pad + root.contentHeight() + pad + card.borderBottom

                    Repeater {
                        id: rowsRepeater
                        model: root.rows

                        delegate: Item {
                            id: rowItem
                            required property int index
                            required property var modelData
                            property int rank: root.rankOf(index)
                            property bool shown: false
                            property real shimmerSweep: -0.25
                            property bool shimmerActive: false
                            width: rowContent.implicitWidth
                            height: rowContent.implicitHeight
                            x: Math.round((card.width - width) / 2)
                            y: card.borderTop + pad + root.rowY(index)
                            opacity: shown ? (rank === 0 ? 1.0 : rank === 1 ? 0.75 : 0.5) : 0

                            Behavior on opacity {
                                NumberAnimation { duration: 220; easing.type: Easing.OutCubic }
                            }

                            Behavior on y {
                                NumberAnimation { duration: 200; easing.type: Easing.OutCubic }
                            }

                            Row {
                                id: rowContent
                                spacing: Style.space(6)

                                Text {
                                    visible: rank >= 1
                                    text: "›"
                                    color: Color.accent
                                    font.family: Style.font.family
                                    font.pixelSize: root.rankPixelSize(rank)
                                    font.bold: true
                                    anchors.verticalCenter: parent.verticalCenter
                                    textFormat: Text.PlainText
                                }

                                Text {
                                    text: root.maskedText(modelData.text)
                                    color: Color.popups.text
                                    font.family: Style.font.family
                                    font.pixelSize: root.rankPixelSize(rank)
                                    font.bold: rank === 0
                                    font.letterSpacing: rank === 0 ? 1.4 : 1.0
                                    font.capitalization: Font.AllUppercase
                                    anchors.verticalCenter: parent.verticalCenter
                                    textFormat: Text.PlainText
                                    layer.enabled: rank === 0 && rowItem.shimmerActive
                                    layer.effect: ShaderEffect {
                                        fragmentShader: Qt.resolvedUrl("shimmer.frag.qsb")
                                        property real sweep: rowItem.shimmerSweep
                                        property real bandWidth: 0.16
                                        property real strength: 0.9
                                    }
                                }
                            }

                            SequentialAnimation {
                                id: shimmerAnim
                                running: false
                                PauseAnimation { duration: 160 }
                                ScriptAction { script: rowItem.shimmerActive = true }
                                NumberAnimation {
                                    target: rowItem
                                    property: "shimmerSweep"
                                    from: -0.25
                                    to: 1.4
                                    duration: 700
                                    easing.type: Easing.OutCubic
                                }
                                ScriptAction { script: rowItem.shimmerActive = false }
                            }

                            Component.onCompleted: {
                                rowItem.shown = true
                                if (rowItem.rank === 0) shimmerAnim.restart()
                            }

                            onModelDataChanged: {
                                if (rowItem.rank === 0) shimmerAnim.restart()
                            }
                        }
                    }
                }
            }
        }
    }
}