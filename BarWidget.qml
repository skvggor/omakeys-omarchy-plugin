import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui

BarWidget {
    id: root
    moduleName: "skvggor.omakeys"

    readonly property var service: root.bar && root.bar.shell ? root.bar.shell.serviceFor("skvggor.omakeys") : null
    readonly property bool enabled: service ? service.enabled : true
    readonly property string statusText: service ? service.statusText : ""
    readonly property bool recording: service ? service.recording : false
    readonly property string fontFamily: root.bar ? root.bar.fontFamily : Style.font.family
    readonly property color foreground: root.bar ? root.bar.foreground : Color.foreground
    readonly property color dim: Qt.darker(foreground, 1.55)
    readonly property bool opened: panelLoader.item ? panelLoader.item.opened === true : false

    function open() {
        if (panelLoader.item) panelLoader.item.open()
    }

    function close() {
        if (panelLoader.item) panelLoader.item.close()
    }

    function toggle() {
        if (panelLoader.item) panelLoader.item.toggle()
    }

    function injectPanel() {
        var target = panelLoader.item
        if (!target) return
        if ("bar" in target) target.bar = root.bar
        if ("settings" in target) target.settings = root.settings
        if ("service" in target) target.service = root.service
        if ("anchorItem" in target) target.anchorItem = button
        if ("hostWidget" in target) target.hostWidget = root
    }

    implicitWidth: Style.space(26)
    implicitHeight: button.implicitHeight

    onBarChanged: {
        injectPanel()
        if (root.service) root.service.settings = root.settings
    }
    onSettingsChanged: {
        injectPanel()
        if (root.service) root.service.settings = root.settings
    }

    Loader {
        id: panelLoader
        active: true
        source: Qt.resolvedUrl("Panel.qml")
        visible: false
        onLoaded: {
            root.injectPanel()
            Qt.callLater(root.injectPanel)
        }
    }

    IpcHandler {
        target: "skvggor.omakeys"

        function state(): string {
            var snapshot = {
                "enabled": root.enabled,
                "recording": root.recording,
                "status": root.statusText
            }
            return JSON.stringify(snapshot)
        }

        function open() { root.open() }
        function close() { root.close() }
        function toggle() { root.toggle() }
    }

    BarIconButton {
        id: button
        anchors.fill: parent
        bar: root.bar
        tooltipText: {
            if (!root.enabled) return "OMAKEYS: DISABLED"
            if (!root.recording) return "OMAKEYS: " + root.statusText
            return "OMAKEYS: RECORDING"
        }
        onPressed: function(buttonCode) {
            if (buttonCode === Qt.MiddleButton && root.service)
                root.service.refresh()
            else
                root.toggle()
        }

        iconComponent: Component {
            Item {
                Text {
                    anchors.centerIn: parent
                    text: "󰌌"
                    color: !root.enabled ? root.dim : (root.recording ? root.foreground : root.dim)
                    font.family: root.fontFamily
                    font.pixelSize: Style.font.icon
                }
            }
        }
    }
}