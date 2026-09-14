import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui

Panel {
    id: root

    property var anchorItem: null
    property var hostWidget: null
    property var service: null

    readonly property color foreground: bar ? bar.foreground : Color.foreground
    readonly property color dim: Qt.darker(foreground, 1.55)
    readonly property string fontFamily: bar ? bar.fontFamily : Style.font.family
    readonly property string overlayStatus: service
        ? (service.overlayEnabled ? "OVERLAY ON" : "OVERLAY OFF")
        : "SERVICE UNAVAILABLE"
    readonly property var activePhrases: [
        "Pounding keys",
        "Tapping letters",
        "Ramping combos",
        "Striking chords",
        "Reading keystrokes",
        "Witnessing presses",
        "Catching taps",
        "Decoding strokes",
        "Spying on typing"
    ]
    property int phraseIndex: 0
    readonly property string heroPhraseText: activePhrases[phraseIndex % activePhrases.length]

    moduleName: "skvggor.omakeys"
    manageIpc: false

    function open() {
        root.controller.show()
    }

    function close() {
        root.controller.hide()
    }

    function toggle() {
        if (opened === true) root.controller.hide()
        else root.controller.show()
    }

    function setSetting(key, value) {
        settingProcess.command = [
            "omarchy", "bar", "set", "skvggor.omakeys",
            key, value ? "true" : "false", "--json"
        ]
        settingProcess.running = true
    }

    Process {
        id: settingProcess
        onExited: function(exitCode) {
            if (exitCode !== 0) console.warn("omakeys: omarchy bar set failed with code " + exitCode)
        }
    }

    KeyboardPanel {
        id: panel
        anchorItem: root.anchorItem
        owner: root.hostWidget || root
        bar: root.bar
        open: root.opened
        focusTarget: keyCatcher
        contentWidth: panel.fittedContentWidth(Style.space(300))
        contentHeight: panel.fittedContentHeight(column.implicitHeight)

        PanelKeyCatcher {
            id: keyCatcher
            anchors.fill: parent
            onCloseRequested: root.close()

            Column {
                id: column
                width: parent.width
                spacing: Style.space(12)

                PanelHero {
                    id: hero
                    width: parent.width
                    title: "OmaKeys"
                    meta: root.service && root.service.enabled ? root.heroPhraseText : "PLUGIN DISABLED"
                    foreground: root.foreground
                    fontFamily: root.fontFamily
                    iconOpacity: root.service && root.service.enabled ? 1.0 : 0.5
                    iconComponent: Component {
                        Text {
                            text: "󰌌"
                            color: root.foreground
                            font.family: root.fontFamily
                            font.pixelSize: Style.font.display
                        }
                    }

                    trailingControl: Component {
                        ToggleSwitch {
                            checked: root.service ? root.service.enabled : true
                            foreground: hero.foreground
                            onToggled: root.setSetting("enabled", !root.service.enabled)
                        }
                    }
                }

                PanelSeparator {
                    foreground: root.foreground
                }

                Item {
                    width: parent.width
                    height: recPill.height

                    Text {
                        text: root.overlayStatus
                        color: root.service && root.service.overlayEnabled ? root.foreground : root.dim
                        font.family: root.fontFamily
                        font.pixelSize: Style.font.body
                        anchors.left: parent.left
                        anchors.verticalCenter: parent.verticalCenter
                    }

                    Rectangle {
                        id: recPill
                        height: Style.space(18)
                        width: Style.space(7) + dot.width + Style.space(6) + Math.ceil(recLabel.implicitWidth) + Style.space(8)
                        radius: height / 2
                        color: root.service && root.service.recording ? Util.alpha(Color.accent, 0.14) : Util.alpha(Color.foreground, 0.1)
                        border.width: 1
                        border.color: root.service && root.service.recording ? Util.alpha(Color.accent, 0.4) : Util.alpha(Color.foreground, 0.25)
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter

                        Rectangle {
                            id: dot
                            anchors.left: parent.left
                            anchors.leftMargin: Style.space(7)
                            anchors.verticalCenter: parent.verticalCenter
                            width: Style.space(6)
                            height: width
                            radius: width / 2
                            color: root.service && root.service.recording ? Color.accent : root.dim
                        }

                        Text {
                            id: recLabel
                            anchors.left: dot.right
                            anchors.leftMargin: Style.space(6)
                            anchors.verticalCenter: parent.verticalCenter
                            text: root.service && root.service.recording ? "REC" : "OFF"
                            color: root.service && root.service.recording ? Color.accent : root.dim
                            font.family: root.fontFamily
                            font.pixelSize: Style.font.caption
                            font.bold: true
                            font.letterSpacing: 1.2
                        }
                    }
                }

                Column {
                    width: parent.width
                    spacing: Style.space(8)
                    visible: !root.service || !root.service.recording

                    Text {
                        width: parent.width
                        text: root.service ? root.service.statusText : "DAEMON NOT CONNECTED"
                        color: Color.urgent
                        font.family: root.fontFamily
                        font.pixelSize: Style.font.caption
                        font.capitalization: Font.AllUppercase
                        wrapMode: Text.WordWrap
                    }

                    Text {
                        width: parent.width
                        visible: root.service && root.service.daemonMissing
                        text: "BUILD AND GRANT EVDEV ACCESS (NO LOGOUT NEEDED):" + "\nCD ~/.CONFIG/OMARCHY/PLUGINS/SKVGGOR.OMAKEYS && ./BIN/OMARCHY-INSTALL-OMAKEYS --INSTALL"
                        color: root.dim
                        font.family: root.fontFamily
                        font.pixelSize: Style.font.caption
                        wrapMode: Text.WordWrap
                    }
                }

                Text {
                    width: parent.width
                    visible: root.service && root.service.recording
                    text: root.service.comboText === "" ? "WAITING FOR INPUT…" : "NOW HOLDING: " + root.service.comboText
                    color: root.foreground
                    font.family: root.fontFamily
                    font.pixelSize: Style.font.body
                    font.bold: true
                    font.capitalization: Font.AllUppercase
                    wrapMode: Text.WordWrap
                }

                Column {
                    width: parent.width
                    spacing: Style.space(2)
                    visible: root.service ? root.service.historyChips.length > 0 : false

                    Text {
                        text: "RECENT"
                        color: root.dim
                        font.family: root.fontFamily
                        font.pixelSize: Style.font.caption - 2
                        font.letterSpacing: 1.2
                        font.capitalization: Font.AllUppercase
                    }

                    Repeater {
                        model: root.service ? root.service.historyChips.slice(0, 8) : []
                        delegate: Text {
                            width: parent.width
                            text: modelData.text
                            color: root.foreground
                            font.family: root.fontFamily
                            font.pixelSize: Style.font.caption
                            font.capitalization: Font.AllUppercase
                            elide: Text.ElideRight
                        }
                    }
                }

                PanelSeparator {
                    foreground: root.foreground
                }

                Text {
                    text: "BEHAVIOR"
                    color: root.dim
                    font.family: root.fontFamily
                    font.pixelSize: Style.font.caption - 2
                    font.letterSpacing: 1.2
                    font.capitalization: Font.AllUppercase
                }

                Toggle {
                    width: parent.width
                    label: "HIDE CHARACTERS"
                    description: "Mask typed letters, digits and symbols (privacy)"
                    checked: root.service ? root.service.hideCharacterKeys : false
                    foreground: root.foreground
                    fontFamily: root.fontFamily
                    onClicked: root.setSetting("hideCharacters", !root.service.hideCharacterKeys)
                }
            }
        }
    }

    Timer {
        id: phraseTimer
        interval: 2800
        running: root.opened && root.service && root.service.enabled
        repeat: true
        onTriggered: phraseSwap.restart()
    }

    SequentialAnimation {
        id: phraseSwap
        PropertyAnimation {
            target: hero
            property: "metaOpacity"
            to: 0.0
            duration: 180
            easing.type: Easing.OutQuad
        }
        ScriptAction {
            script: root.phraseIndex = (root.phraseIndex + 1) % root.activePhrases.length
        }
        PropertyAnimation {
            target: hero
            property: "metaOpacity"
            to: 1.0
            duration: 260
            easing.type: Easing.InQuad
        }
    }
}