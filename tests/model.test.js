'use strict'

const assert = require('node:assert/strict')
const { test } = require('node:test')

const Model = require('../Model.js')

test('parseState returns a clean default for empty input', () => {
  const state = Model.parseState('')
  assert.deepEqual(state.keys, [])
  assert.deepEqual(state.mouse, [])
  assert.equal(state.ok, false)
})

test('parseState maps the daemon JSON into typed fields', () => {
  const raw = JSON.stringify({
    version: 1,
    ok: true,
    error: '',
    keys: ['Ctrl', 'a'],
    mouse: ['LMB'],
    active_at: 123,
    events: [{ text: 'Ctrl+a', at: 123 }]
  })
  const state = Model.parseState(raw)
  assert.equal(state.ok, true)
  assert.deepEqual(state.keys, ['CTRL', 'A'])
  assert.deepEqual(state.mouse, ['LMB'])
  assert.equal(state.activeAt, 123)
  assert.equal(state.events[0].text, 'CTRL+A')
})

test('parseState keeps a camelCase activeAt when the daemon starts using it', () => {
  const state = Model.parseState(JSON.stringify({ activeAt: 456 }))
  assert.equal(state.activeAt, 456)
})

test('parseState survives malformed JSON and reports an error', () => {
  const state = Model.parseState('{not json')
  assert.equal(state.ok, false)
  assert.equal(state.error, 'Failed to parse daemon state')
})

test('surfaceChips lists keys first, then mouse when enabled', () => {
  const state = Model.parseState('""') || {}
  const checked = {
    ok: true,
    keys: ['Alt', 'a'],
    mouse: ['RMB']
  }
  const withMouse = Model.surfaceChips(checked, { showMouse: true })
  assert.deepEqual(withMouse.map(c => c.text), ['ALT', 'A', 'RMB'])
  const withoutMouse = Model.surfaceChips(checked, { showMouse: false })
  assert.deepEqual(withoutMouse.map(c => c.text), ['ALT', 'A'])
})

test('surfaceChips honours the maxKeys limit', () => {
  const checked = { ok: true, keys: ['1', '2', '3', '4', '5'], mouse: [] }
  const chips = Model.surfaceChips(checked, { maxKeys: 3 })
  assert.equal(chips.length, 3)
})

test('surfaceChips and historyChips are empty while the plugin is disabled', () => {
  const checked = {
    ok: true,
    keys: ['Alt', 'a'],
    mouse: ['RMB'],
    events: [{ text: 'W', at: Date.now() }]
  }
  assert.deepEqual(Model.surfaceChips(checked, { enabled: false }), [])
  assert.deepEqual(Model.historyChips(checked, { enabled: false }), [])
  assert.equal(Model.comboText(checked, { enabled: false }), '')
})

test('settings helpers clamp values to sane ranges', () => {
  assert.equal(Model.maxKeys({ maxKeys: 99 }), Model.MAX_KEYS_LIMIT)
  assert.equal(Model.maxKeys({ maxKeys: -2 }), 0)
  assert.equal(Model.overlayVerticalPercent({ overlayVerticalPercent: 5 }), 20)
  assert.equal(Model.overlayVerticalPercent({ overlayVerticalPercent: 95 }), 90)
  assert.equal(Model.isOverlayEnabled({ overlayEnabled: true }), true)
  assert.equal(Model.isOverlayEnabled({ overlayEnabled: false }), false)
})

test('parseState uppercases all display text from the daemon', () => {
  const state = Model.parseState(JSON.stringify({
    ok: true,
    keys: ['Enter', 'Shift', 'a'],
    mouse: ['Scroll Up'],
    events: [{ text: 'Super+Tab', at: 1 }]
  }))
  assert.deepEqual(state.keys, ['ENTER', 'SHIFT', 'A'])
  assert.deepEqual(state.mouse, ['SCROLL UP'])
  assert.equal(state.events[0].text, 'SUPER+TAB')
})

test('comboText joins surface chips with a plus separator', () => {
  const checked = { ok: true, keys: ['Ctrl', 'Alt', 'a'], mouse: [] }
  assert.equal(Model.comboText(checked, {}), 'CTRL+ALT+A')
})

test('daemonStatusText surfaces the error message', () => {
  const state = { ok: false, error: 'no readable input devices' }
  assert.equal(Model.daemonStatusText(state), 'NO READABLE INPUT DEVICES')
  assert.equal(Model.daemonStatusText({ ok: true }), 'RECORDING')
})

test('enabled helpers default to sensible values', () => {
  assert.equal(Model.isPluginEnabled({}), true)
  assert.equal(Model.isPluginEnabled({ enabled: false }), false)
  assert.equal(Model.isCharHidingEnabled({}), false)
  assert.equal(Model.isCharHidingEnabled({ hideCharacters: true }), true)
})

test('normalizeText turns glyphs into plain ascii text', () => {
  assert.equal(Model.normalizeText('Scroll ↑'), 'SCROLL UP')
  assert.equal(Model.normalizeText('×'), '*')
  assert.equal(Model.normalizeText('KP_Subtract'), 'KP_SUBTRACT')
  assert.equal(Model.normalizeText('Ctrl+a'), 'CTRL+A')
})

test('surfaceChips masks printable characters but keeps labels when hiding', () => {
  const checked = { ok: true, keys: ['Ctrl', 'a', '*'], mouse: ['LMB'] }
  const masked = Model.surfaceChips(checked, { hideCharacters: true })
  assert.deepEqual(masked.map(c => c.text), ['CTRL', '•', '•', 'LMB'])
  const visible = Model.surfaceChips(checked, { hideCharacters: false })
  assert.deepEqual(visible.map(c => c.text), ['CTRL', 'A', '*', 'LMB'])
})

test('historyChips mask single characters inside combos', () => {
  const now = Date.now()
  const state = { ok: true, events: [{ text: 'Ctrl+a', at: now }, { text: 'ENTER', at: now }] }
  const masked = Model.historyChips(state, { hideCharacters: true })
  assert.deepEqual(masked.map(c => c.text), ['CTRL+•', 'ENTER'])
  const visible = Model.historyChips(state, { hideCharacters: false })
  assert.deepEqual(visible.map(c => c.text), ['CTRL+A', 'ENTER'])
})

test('a tapped modifier stays available as the most recent event', () => {
  const raw = JSON.stringify({
    version: 1,
    ok: true,
    keys: [],
    mouse: [],
    active_at: Date.now(),
    events: [{ text: 'Caps', at: Date.now() }]
  })
  const state = Model.parseState(raw)
  assert.deepEqual(Model.comboText(state, {}), '')
  assert.equal(state.events[0].text, 'CAPS')
  assert.equal(Model.historyChips(state, {})[0].text, 'CAPS')
})