'use strict'

const assert = require('node:assert/strict')
const { test } = require('node:test')
const fs = require('node:fs')
const path = require('node:path')

const root = path.join(__dirname, '..')

function manifest() {
  return JSON.parse(fs.readFileSync(path.join(root, 'manifest.json'), 'utf8'))
}

function cargoVersion() {
  const cargo = fs.readFileSync(path.join(root, 'Cargo.toml'), 'utf8')
  const match = cargo.match(/^version\s*=\s*"([^"]+)"/m)
  assert.ok(match, 'Cargo.toml must declare a version')
  return match[1]
}

test('manifest.json and Cargo.toml declare the same version', () => {
  assert.equal(manifest().version, cargoVersion())
})

test('versions are plain semver', () => {
  for (const version of [manifest().version, cargoVersion()]) {
    assert.match(version, /^\d+\.\d+\.\d+$/, `unexpected version: ${version}`)
  }
})

test('manifest exposes a service, a bar widget, and an overlay panel', () => {
  const data = manifest()
  assert.deepEqual(data.kinds, ['service', 'bar-widget', 'panel'])
  assert.equal(data.keepLoaded, true)
  assert.equal(data.entryPoints.service, 'Service.qml')
  assert.equal(data.entryPoints.barWidget, 'BarWidget.qml')
  assert.equal(data.entryPoints.panel, 'Overlay.qml')
})

test('manifest defaults match the schema defaultValues', () => {
  const data = manifest()
  const widget = data.barWidget || {}
  for (const entry of widget.schema || []) {
    if (!(entry.key in widget.defaults)) continue
    assert.deepEqual(
      widget.defaults[entry.key],
      entry.defaultValue,
      `default for ${entry.key} disagrees with the schema`
    )
  }
})