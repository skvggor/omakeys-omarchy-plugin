import "Model.js" as Model
import QtQuick
import Quickshell
import Quickshell.Io

Item {
    id: root

    property var settings: ({})
    property alias state: stateHook.value
    property bool daemonMissing: false
    property int restartAttempts: 0
    property bool pendingEnableWrite: false

    readonly property string home: Quickshell.env("HOME")
    readonly property string stateDir: home + "/.local/state/omarchy/current/plugins/skvggor.omakeys"
    readonly property string statePath: stateDir + "/keys.json"
    readonly property string enabledPath: stateDir + "/enabled"
    readonly property string daemonPath: String(Qt.resolvedUrl("bin/omakeys-daemon")).replace(/^file:\/\//, "")

    readonly property var historyChips: Model.historyChips(state, root.settings)
    readonly property string comboText: Model.comboText(state, root.settings)
    readonly property bool enabled: Model.isPluginEnabled(root.settings)
    readonly property bool hideCharacterKeys: Model.isCharHidingEnabled(root.settings)
    readonly property bool overlayEnabled: Model.isOverlayEnabled(root.settings) && root.enabled
    readonly property int overlayVerticalPercent: Model.overlayVerticalPercent(root.settings)
    readonly property bool recording: root.enabled && state && state.ok === true
    readonly property string statusText: root.enabled ? Model.daemonStatusText(state) : "DISABLED"

    onSettingsChanged: root.applySettings()
    onEnabledChanged: root.applySettings()

    function refresh() {
        stateFile.reload()
    }

    function start() {
        if (!daemonProcess.running && !root.daemonMissing) {
            restartAttempts = 0
            daemonMissing = false
            daemonProcess.command = [root.daemonPath, "--state", root.statePath, "--enable", root.enabledPath]
            daemonProcess.running = true
        }
    }

    function setEnableFlag() {
        var value = root.enabled ? "true" : "false"
        if (flagProcess.running) {
            root.pendingEnableWrite = true
            return
        }
        flagProcess.command = [
            "bash", "-c",
            "umask 077 && mkdir -p \"$1\" && printf '%s' \"$2\" > \"$3\"",
            "_", root.stateDir, value, root.enabledPath
        ]
        flagProcess.running = true
    }

    function applySettings() {
        root.setEnableFlag()
        if (!root.enabled) {
            restartTimer.stop()
            return
        }
        if (!daemonProcess.running && !root.daemonMissing) root.start()
    }

    Item {
        id: stateHook
        property var value: Model.defaultState()
    }

    function applyState(raw) {
        stateHook.value = Model.parseState(raw)
    }

    FileView {
        id: stateFile
        path: root.statePath
        watchChanges: true
        printErrors: false
        onLoaded: root.applyState(text())
        onFileChanged: reload()
        onLoadFailed: root.applyState("")
    }

    Process {
        id: daemonProcess

        running: false
        command: []
        onExited: function(exitCode) {
            root.restartAttempts += 1
            if (root.restartAttempts <= 5) {
                if (root.enabled) restartTimer.restart()
            }
        }
    }

    Process {
        id: flagProcess
        running: false
        command: []
        onExited: {
            if (root.pendingEnableWrite) {
                root.pendingEnableWrite = false
                root.setEnableFlag()
            }
        }
    }

    FileView {
        id: daemonFile
        path: root.daemonPath
        watchChanges: true
        printErrors: false
        onLoaded: {
            root.daemonMissing = false
            root.start()
        }
        onLoadFailed: {
            root.daemonMissing = true
        }
    }

    Timer {
        id: restartTimer
        interval: 1500
        repeat: false
        onTriggered: root.start()
    }

    Timer {
        id: startTimer
        interval: 400
        repeat: false
        triggeredOnStart: true
        onTriggered: {
            root.setEnableFlag()
            if (!root.daemonMissing) root.start()
        }
    }
}