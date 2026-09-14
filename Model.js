const DEFAULT_OVERLAY_PERCENT = 85
const DEFAULT_MAX_KEYS = 6
const MAX_KEYS_LIMIT = 24
const MIN_KEYS_LIMIT = 0

function upper(value) {
  return String(value || '').toUpperCase()
}

const GLYPH_REPLACEMENTS = {
  "\u2191": "UP",
  "\u2193": "DOWN",
  "\u2190": "LEFT",
  "\u2192": "RIGHT",
  "\u00d7": "*",
  "\u00f7": "/",
  "\u2212": "-",
  "\uff0b": "+"
}

function normalizeText(value) {
  var text = String(value || '').toUpperCase()
  var keys = Object.keys(GLYPH_REPLACEMENTS)
  for (var i = 0; i < keys.length; i++) {
    text = text.split(keys[i]).join(GLYPH_REPLACEMENTS[keys[i]])
  }
  text = text.replace(/[\u2000-\u200b\u202f\ufeff]/g, "")
  return text
}

function isMaskableChar(text) {
  if (typeof text !== 'string' || text.length !== 1) return false
  return /\S/.test(text)
}

function maskChars(text) {
  return String(text || '').split('+').map(function (part) {
    return isMaskableChar(part) ? '\u2022' : part
  }).join('+')
}

function chipText(raw, settings) {
  var text = normalizeText(raw)
  if (isCharHidingEnabled(settings) && isMaskableChar(text)) text = '\u2022'
  return text
}

function defaultState() {
  return {
    version: 1,
    ok: false,
    error: "",
    keys: [],
    mouse: [],
    activeAt: 0,
    events: []
  }
}

function parseState(raw) {
  const text = String(raw || "").trim()
  if (text === "") return defaultState()
  try {
    const parsed = JSON.parse(text)
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) return defaultState()
    parsed.ok = parsed.ok === true
    parsed.error = String(parsed.error || "")
    parsed.activeAt = Number(parsed.activeAt != null ? parsed.activeAt : parsed.active_at) || 0
    parsed.keys = Array.isArray(parsed.keys) ? parsed.keys.map(normalizeText) : []
    parsed.mouse = Array.isArray(parsed.mouse) ? parsed.mouse.map(normalizeText) : []
    parsed.events = Array.isArray(parsed.events) ? parsed.events.map(function (entry) {
      return {
        text: normalizeText(entry && entry.text || ""),
        at: Number(entry && entry.at || 0)
      }
    }) : []
    return parsed
  } catch (error) {
    const failed = defaultState()
    failed.error = "Failed to parse daemon state"
    return failed
  }
}

function isPlainSettings(value) {
  return value && typeof value === "object" && !Array.isArray(value)
}

function booleanSetting(settings, key, fallback) {
  if (!isPlainSettings(settings) || settings[key] === undefined) return fallback
  return settings[key] === true || settings[key] === "true"
}

function numberSetting(settings, key, fallback) {
  if (!isPlainSettings(settings) || settings[key] === undefined) return fallback
  const value = Number(settings[key])
  return isFinite(value) ? value : fallback
}

function maxKeys(settings) {
  const value = Math.floor(numberSetting(settings, "maxKeys", DEFAULT_MAX_KEYS))
  if (!isFinite(value)) return DEFAULT_MAX_KEYS
  return Math.max(MIN_KEYS_LIMIT, Math.min(MAX_KEYS_LIMIT, value))
}

function overlayVerticalPercent(settings) {
  const value = numberSetting(settings, "overlayVerticalPercent", DEFAULT_OVERLAY_PERCENT)
  if (!isFinite(value)) return DEFAULT_OVERLAY_PERCENT
  return Math.max(20, Math.min(90, value))
}

function isOverlayEnabled(settings) {
  return booleanSetting(settings, "overlayEnabled", true)
}

function isPluginEnabled(settings) {
  return booleanSetting(settings, "enabled", true)
}

function isCharHidingEnabled(settings) {
  return booleanSetting(settings, "hideCharacters", false)
}

function isMouseEnabled(settings) {
  return booleanSetting(settings, "showMouse", true)
}

function surfaceChips(state, settings) {
  if (!isPluginEnabled(settings)) return []
  const chips = []
  const stateKeys = state && Array.isArray(state.keys) ? state.keys : []
  const limit = maxKeys(settings)
  for (let index = 0; index < stateKeys.length && index < limit; index++) {
    chips.push({ isMouse: false, text: chipText(stateKeys[index], settings) })
  }
  if (isMouseEnabled(settings)) {
    const stateMouse = state && Array.isArray(state.mouse) ? state.mouse : []
    for (let index = 0; index < stateMouse.length; index++) {
      chips.push({ isMouse: true, text: normalizeText(stateMouse[index]) })
    }
  }
  return chips
}

function historyChips(state, settings) {
  if (!isPluginEnabled(settings)) return []
  const entries = state && Array.isArray(state.events) ? state.events : []
  const limit = maxKeys(settings)
  const now = Date.now()
  const chips = []
  for (let index = 0; index < entries.length && chips.length < limit; index++) {
    const entry = entries[index]
    if (!entry || !entry.text) continue
    let text = normalizeText(entry.text)
    if (isCharHidingEnabled(settings)) text = maskChars(text)
    chips.push({ text: text, ageMs: now - Number(entry.at || 0) })
  }
  return chips
}

function comboText(state, settings) {
  return surfaceChips(state, settings).map(function (chip) { return chip.text }).join("+")
}

function daemonStatusText(state) {
  if (!state || !state.ok) {
    const detail = state && state.error ? String(state.error) : ""
    if (detail) return upper(detail)
    return "DAEMON NOT CONNECTED"
  }
  return "RECORDING"
}

if (typeof module !== "undefined") {
  module.exports = {
    DEFAULT_OVERLAY_PERCENT,
    DEFAULT_MAX_KEYS,
    MAX_KEYS_LIMIT,
    MIN_KEYS_LIMIT,
    upper,
    normalizeText,
    maskChars,
    isMaskableChar,
    defaultState,
    parseState,
    isPlainSettings,
    booleanSetting,
    numberSetting,
    maxKeys,
    overlayVerticalPercent,
    isOverlayEnabled,
    isPluginEnabled,
    isCharHidingEnabled,
    isMouseEnabled,
    surfaceChips,
    historyChips,
    comboText,
    daemonStatusText
  }
}