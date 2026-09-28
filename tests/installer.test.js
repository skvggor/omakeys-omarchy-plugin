'use strict'

const assert = require('node:assert/strict')
const { test, after } = require('node:test')
const fs = require('node:fs')
const os = require('node:os')
const path = require('node:path')
const crypto = require('node:crypto')
const { spawnSync } = require('node:child_process')

const PROJECT_ROOT = path.join(__dirname, '..')
const SCRIPT_PATH = path.join(PROJECT_ROOT, 'bin', 'omarchy-install-omakeys')
const CHAIN_PATH = path.join(PROJECT_ROOT, 'bin', 'sigstore-fulcio-chain.pem')
const PIN_PATH = path.join(PROJECT_ROOT, 'bin', 'omakeys-daemon-x86_64-linux-gnu.sha256')
const FIXTURE_PATH = path.join(__dirname, 'fixtures', 'v1.0.0-attestation-bundle.json')
const SCRIPT_SOURCE = fs.readFileSync(SCRIPT_PATH, 'utf8')

const REPO = SCRIPT_SOURCE.match(/^REPO="([^"]+)"$/m)[1]
const SIGNER_WORKFLOW = SCRIPT_SOURCE.match(/^SIGNER_WORKFLOW="([^"]+)"$/m)[1]
const OIDC_ISSUER = SCRIPT_SOURCE.match(/^OIDC_ISSUER="([^"]+)"$/m)[1]
const ACCEPTED_COMMITS = [
  ...SCRIPT_SOURCE.match(/^ACCEPTED_SOURCE_COMMITS=\((.*)\)$/m)[1].matchAll(/"([0-9a-f]{40})"/g),
].map((match) => match[1])

const BASH = findExecutable('bash')
const OPENSSL = findExecutable('openssl')
const PAYLOAD_TYPE = 'application/vnd.in-toto+json'
const ASSET_NAME = 'omakeys-daemon-x86_64-linux-gnu'
const ASSET_BYTES = Buffer.from('fake omakeys daemon binary\n')
const ASSET_SHA256 = crypto.createHash('sha256').update(ASSET_BYTES).digest('hex')
const KNOWN_COMMIT = 'a'.repeat(40)
const SIGNER_SUBJECT = `URI:https://github.com/${REPO}/${SIGNER_WORKFLOW}@refs/tags/v1.0.0`
const TRACE_PATTERN = /^\++L(\d+):/
const SYSTEM_ROOT_CERTIFICATE = findSystemRootCertificate()
const REAL_TOOLS = [
  'awk',
  'base64',
  'basename',
  'cat',
  'chmod',
  'cp',
  'cut',
  'dirname',
  'grep',
  'head',
  'jq',
  'mkdir',
  'mktemp',
  'rm',
  'sed',
  'sha256sum',
  'tr',
  'wc',
]

const tracedLines = new Set()
const tempRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'omakeys-installer-'))
let sandboxSequence = 0
let sharedAuthority = null
let foreignAuthority = null
const runningAsRoot = typeof process.getuid === 'function' && process.getuid() === 0
const skipReason = runningAsRoot ? 'installer expects a non-root user' : false

after(() => {
  fs.rmSync(tempRoot, { recursive: true, force: true })
  if (skipReason) return
  const executable = executableLines(SCRIPT_SOURCE)
  const covered = executable.filter((line) => tracedLines.has(line))
  const ratio = covered.length / executable.length
  const uncovered = executable.filter((line) => !tracedLines.has(line))
  assert.ok(
    ratio >= 0.8,
    `installer line coverage ${(ratio * 100).toFixed(1)}% is below 80% (${covered.length}/${executable.length}); ` +
      `uncovered lines: ${uncovered.join(', ')}`
  )
})

function findExecutable(name) {
  for (const directory of (process.env.PATH || '').split(path.delimiter)) {
    const candidate = path.join(directory, name)
    try {
      fs.accessSync(candidate, fs.constants.X_OK)
      return candidate
    } catch {
      continue
    }
  }
  return null
}

function isStructural(trimmed) {
  return /^(fi|esac|done|else|elif|then|do|;;)(\b|$)/.test(trimmed) || trimmed === '}' || trimmed === '('
}

function executableLines(source) {
  const lines = source.split('\n')
  const executable = []
  let quote = null
  let continued = false
  let depth = 0

  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index]
    const trimmed = line.trim()
    const startsLogicalLine = !continued && quote === null && depth === 0

    if (startsLogicalLine && trimmed && !trimmed.startsWith('#') && !isStructural(trimmed)) {
      executable.push(index + 1)
    }

    const trackState = quote !== null || !trimmed.startsWith('#')
    if (!trackState) continue

    for (let position = 0; position < line.length; position += 1) {
      const character = line[position]
      if (quote === "'") {
        if (character === "'") quote = null
        continue
      }
      if (quote === '"') {
        if (character === '\\') position += 1
        else if (character === '"') quote = null
        continue
      }
      if (character === '\\') position += 1
      else if (character === "'" || character === '"') quote = character
      else if (character === '(') depth += 1
      else if (character === ')' && depth > 0) depth -= 1
    }
    continued = line.endsWith('\\')
  }

  return executable
}

function runOpenssl(argumentsList, cwd) {
  const result = spawnSync(OPENSSL, argumentsList, { cwd, encoding: 'utf8' })
  if (result.error) throw result.error
  if (result.status !== 0) {
    throw new Error(`openssl ${argumentsList.join(' ')} exited ${result.status}: ${result.stderr || result.stdout}`)
  }
  return result
}

function findSystemRootCertificate() {
  const directory = '/etc/ssl/certs'
  let entries
  try {
    entries = fs.readdirSync(directory)
  } catch {
    return null
  }
  const candidate = entries.filter((entry) => entry.endsWith('.pem')).sort()[0]
  return candidate ? path.join(directory, candidate) : null
}

function createCertificateAuthority(directory, commonName) {
  fs.mkdirSync(directory, { recursive: true })
  const keyPath = path.join(directory, 'ca.key')
  const certificatePath = path.join(directory, 'ca.pem')
  runOpenssl(
    [
      'req', '-x509', '-newkey', 'ec', '-pkeyopt', 'ec_paramgen_curve:prime256v1',
      '-keyout', keyPath, '-out', certificatePath, '-days', '3650', '-nodes',
      '-subj', `/O=sigstore.dev/CN=${commonName}`,
      '-addext', 'basicConstraints=critical,CA:TRUE',
      '-addext', 'keyUsage=critical,keyCertSign,cRLSign',
    ],
    directory
  )
  return { directory, keyPath, certificatePath }
}

function issueSigningCertificate({ directory, authority, subjectAltName, includeOidcIssuer }) {
  const extensionsPath = path.join(directory, 'extensions.cnf')
  const extensions = [
    'basicConstraints=CA:FALSE',
    'keyUsage=critical,digitalSignature',
    `subjectAltName=${subjectAltName}`,
  ]
  if (includeOidcIssuer) extensions.push(`1.3.6.1.4.1.57264.1.1=ASN1:IA5String:${OIDC_ISSUER}`)
  fs.writeFileSync(extensionsPath, `${extensions.join('\n')}\n`)

  const keyPath = path.join(directory, 'signer.key')
  const requestPath = path.join(directory, 'signer.csr')
  const certificatePath = path.join(directory, 'signer.pem')
  runOpenssl(
    [
      'req', '-newkey', 'ec', '-pkeyopt', 'ec_paramgen_curve:prime256v1',
      '-keyout', keyPath, '-out', requestPath, '-nodes',
      '-subj', '/O=sigstore.dev/CN=Attestation Signer',
    ],
    directory
  )
  runOpenssl(
    [
      'x509', '-req', '-in', requestPath,
      '-CA', authority.certificatePath, '-CAkey', authority.keyPath, '-CAcreateserial',
      '-days', '1', '-out', certificatePath, '-extfile', extensionsPath,
    ],
    directory
  )
  return { keyPath, certificatePath }
}

function dsseMessage(payloadType, payload, encoding) {
  const type = Buffer.from(payloadType, 'utf8')
  const body = Buffer.from(payload, 'utf8')
  if (encoding === 'colon') {
    return Buffer.concat([
      Buffer.from('DSSEv1', 'utf8'),
      Buffer.from(String(type.length), 'utf8'),
      Buffer.from(':', 'utf8'),
      type,
      Buffer.from(String(body.length), 'utf8'),
      Buffer.from(':', 'utf8'),
      body,
    ])
  }
  return Buffer.concat([
    Buffer.from('DSSEv1 ', 'utf8'),
    Buffer.from(String(type.length), 'utf8'),
    Buffer.from(' ', 'utf8'),
    type,
    Buffer.from(' ', 'utf8'),
    Buffer.from(String(body.length), 'utf8'),
    Buffer.from(' ', 'utf8'),
    body,
  ])
}

function signMessage(keyPath, message, directory) {
  const messagePath = path.join(directory, 'dsse-message.bin')
  const signaturePath = path.join(directory, 'dsse-signature.bin')
  fs.writeFileSync(messagePath, message)
  runOpenssl(['dgst', '-sha256', '-sign', keyPath, '-out', signaturePath, messagePath], directory)
  return fs.readFileSync(signaturePath)
}

function toDer(certificatePath, directory, outputName = 'certificate.der') {
  const derPath = path.join(directory, outputName)
  runOpenssl(['x509', '-in', certificatePath, '-outform', 'der', '-out', derPath], directory)
  return fs.readFileSync(derPath)
}

function provenanceStatement({ commit = ACCEPTED_COMMITS[0], predicateType = 'https://slsa.dev/provenance/v1', subjectDigest = ASSET_SHA256 } = {}) {
  return {
    _type: 'https://in-toto.io/Statement/v1',
    predicateType,
    subject: [{ name: ASSET_NAME, digest: { sha256: subjectDigest } }],
    predicate: {
      buildDefinition: {
        buildType: 'https://actions.github.io/buildtypes/workflow/v1',
        externalParameters: { workflow: { repository: `https://github.com/${REPO}` } },
        resolvedDependencies: [{ uri: `git+https://github.com/${REPO}`, digest: { gitCommit: commit } }],
      },
      runDetails: { builder: { id: `https://github.com/${REPO}/.github/workflows/release.yml` } },
    },
  }
}

function buildAttestationResponse(settings, directory) {
  if (settings.openssl === 'absent') return { attestations: [] }

  if (!sharedAuthority) {
    sharedAuthority = createCertificateAuthority(path.join(tempRoot, 'authority'), 'sigstore-test')
  }
  const subjectAltName =
    settings.certificate === 'foreignIdentity'
      ? 'URI:https://github.com/attacker/attacker/.github/workflows/release.yml@refs/tags/v9.9.9'
      : SIGNER_SUBJECT
  const certificate = issueSigningCertificate({
    directory,
    authority: sharedAuthority,
    subjectAltName,
    includeOidcIssuer: settings.certificate !== 'withoutIssuer',
  })

  const statement = JSON.stringify(provenanceStatement(settings.statement))
  const signature = signMessage(certificate.keyPath, dsseMessage(PAYLOAD_TYPE, statement, settings.pae), directory)
  if (settings.signature === 'tampered') signature[signature.length - 1] ^= 0xff

  let certificateBytes = toDer(certificate.certificatePath, directory)
  if (settings.certificate === 'systemTrusted') {
    if (!SYSTEM_ROOT_CERTIFICATE) throw new Error('no system root certificate available for this test')
    certificateBytes = toDer(SYSTEM_ROOT_CERTIFICATE, directory, 'system-root.der')
  }

  const payload = settings.payloadEncoding === 'invalid' ? '!!!not-base64!!!' : Buffer.from(statement).toString('base64')
  const signed = settings.signatureEncoding === 'invalid' ? '!!!not-base64!!!' : signature.toString('base64')
  let der = settings.certificateEncoding === 'invalid' ? '!!!not-base64!!!' : certificateBytes.toString('base64')
  if (settings.certificateEncoding === 'garbage') der = Buffer.from('this is not a certificate').toString('base64')

  const envelope = { payloadType: PAYLOAD_TYPE, payload, signatures: [{ sig: signed }] }
  if (settings.payloadType === 'missing') delete envelope.payloadType

  const verificationMaterial = { certificate: { rawBytes: der } }
  if (settings.integratedTime === 'missing') {
    verificationMaterial.tlogEntries = []
  } else if (settings.integratedTime === 'malformed') {
    verificationMaterial.tlogEntries = [{ integratedTime: 'not-a-timestamp' }]
  } else if (settings.integratedTime === 'stale') {
    verificationMaterial.tlogEntries = [{ integratedTime: String(Math.floor(Date.now() / 1000) - 3 * 86400) }]
  } else {
    verificationMaterial.tlogEntries = [{ integratedTime: String(Math.floor(Date.now() / 1000) + 60) }]
  }

  return {
    attestations: [
      {
        bundle: {
          mediaType: 'application/vnd.dev.sigstore.bundle.v0.3+json',
          dsseEnvelope: envelope,
          verificationMaterial,
        },
      },
    ],
  }
}

function writeStub(filePath, body) {
  fs.writeFileSync(filePath, `#!${BASH}\n${body}`)
  fs.chmodSync(filePath, 0o755)
}

function createSandbox(options = {}) {
  const settings = {
    assetCurl: 'ok',
    attestationsCurl: 'ok',
    latestCurl: 'ok',
    manifest: true,
    pin: 'match',
    statement: {},
    arch: 'x86_64',
    sudo: true,
    pkexec: false,
    cargo: false,
    groups: 'sys video',
    daemon: null,
    daemonGroup: 'users',
    version: 'v1.0.0',
    openssl: 'present',
    chain: 'matching',
    certificate: 'valid',
    integratedTime: 'present',
    signature: 'valid',
    payloadType: 'present',
    payloadEncoding: 'valid',
    signatureEncoding: 'valid',
    certificateEncoding: 'valid',
    pae: 'space',
    ...options,
  }

  sandboxSequence += 1
  const base = path.join(tempRoot, `sandbox-${sandboxSequence}`)
  const binDir = path.join(base, 'bin')
  const pluginDir = path.join(base, 'plugin')
  const logDir = path.join(base, 'logs')
  const scriptDir = path.join(pluginDir, 'bin')
  fs.mkdirSync(binDir, { recursive: true })
  fs.mkdirSync(logDir, { recursive: true })
  fs.mkdirSync(scriptDir, { recursive: true })

  for (const tool of REAL_TOOLS) {
    const candidate = findExecutable(tool)
    assert.ok(candidate, `required tool not found on PATH: ${tool}`)
    fs.symlinkSync(candidate, path.join(binDir, tool))
  }
  if (settings.openssl === 'present') fs.symlinkSync(OPENSSL, path.join(binDir, 'openssl'))

  const script = path.join(scriptDir, 'omarchy-install-omakeys')
  fs.copyFileSync(SCRIPT_PATH, script)

  if (settings.manifest) {
    fs.copyFileSync(path.join(PROJECT_ROOT, 'manifest.json'), path.join(pluginDir, 'manifest.json'))
  }

  if (settings.pin === 'match') {
    fs.writeFileSync(path.join(scriptDir, `${ASSET_NAME}.sha256`), `${ASSET_SHA256}  ${ASSET_NAME}\n`)
  } else if (settings.pin === 'mismatch') {
    fs.writeFileSync(path.join(scriptDir, `${ASSET_NAME}.sha256`), `${'0'.repeat(64)}  ${ASSET_NAME}\n`)
  } else if (settings.pin === 'empty') {
    fs.writeFileSync(path.join(scriptDir, `${ASSET_NAME}.sha256`), '\n')
  }

  const assetFile = path.join(base, 'release-asset')
  fs.writeFileSync(assetFile, ASSET_BYTES)
  const attestationFile = path.join(base, 'attestations.json')
  fs.writeFileSync(attestationFile, JSON.stringify(buildAttestationResponse(settings, base)))

  if (settings.openssl === 'present' && settings.chain !== 'missing') {
    const chainCertificate =
      settings.chain === 'foreign'
        ? (foreignAuthority ??= createCertificateAuthority(path.join(tempRoot, 'foreign-authority'), 'unrelated-test'))
            .certificatePath
        : sharedAuthority.certificatePath
    fs.copyFileSync(chainCertificate, path.join(scriptDir, 'sigstore-fulcio-chain.pem'))
  }

  const daemon = path.join(scriptDir, 'omakeys-daemon')
  if (settings.daemon) {
    fs.writeFileSync(daemon, 'previously installed daemon\n')
    fs.chmodSync(daemon, settings.daemon)
  }

  writeStub(path.join(binDir, 'curl'), `
set -euo pipefail
echo "$*" >> "$TEST_LOG_DIR/curl.log"
out=""
url=""
head_request=0
prev=""
for arg in "$@"; do
  if [ "$prev" = "-o" ]; then out="$arg"; fi
  case "$arg" in
    -o) prev="-o" ;;
    --head) head_request=1; prev="" ;;
    http*|https*) url="$arg"; prev="" ;;
    *) prev="" ;;
  esac
done
if [ "$head_request" = "1" ] && [[ "$url" == *"/releases/latest"* ]]; then
  if [ "$CURL_LATEST_MODE" = "empty" ]; then exit 0; fi
  echo "location: https://github.com/$TEST_REPO/releases/tag/v1.0.0"
  exit 0
fi
if [[ "$url" == *"/releases/download/"* ]]; then
  if [ "$CURL_ASSET_MODE" = "fail" ]; then
    echo "curl: (22) The requested URL returned error: 404" >&2
    exit 22
  fi
  cp "$TEST_ASSET_FILE" "$out"
  exit 0
fi
if [[ "$url" == *"/attestations/sha256:"* ]]; then
  if [ "$CURL_ATTEST_MODE" = "fail" ]; then
    echo "curl: (22) The requested URL returned error: 403" >&2
    exit 22
  fi
  if [ "$CURL_ATTEST_MODE" = "empty" ]; then
    echo '{"attestations":[]}' > "$out"
    exit 0
  fi
  cp "$TEST_ATTEST_FILE" "$out"
  exit 0
fi
echo "unexpected curl invocation: $url" >&2
exit 9
`)

  writeStub(path.join(binDir, 'install'), `
set -euo pipefail
echo "$*" >> "$TEST_LOG_DIR/install.log"
mode=""
src=""
dst=""
while [ $# -gt 0 ]; do
  case "$1" in
    -m) mode="$2"; shift 2 ;;
    -o|-g) shift 2 ;;
    *) if [ -z "$src" ]; then src="$1"; else dst="$1"; fi; shift ;;
  esac
done
mkdir -p "$(dirname "$dst")"
cp "$src" "$dst"
chmod "$mode" "$dst"
`)

  writeStub(path.join(binDir, 'uname'), 'echo "$TEST_ARCH"\n')
  writeStub(path.join(binDir, 'stat'), `
case "$2" in
  '%s bytes') echo "123 bytes" ;;
  '%G') echo "$TEST_DAEMON_GROUP" ;;
  *) exit 1 ;;
esac
`)
  writeStub(
    path.join(binDir, 'id'),
    `
case "$1" in
  -nG) echo "$TEST_GROUPS" ;;
  -un) echo "tester" ;;
  *) exit 1 ;;
esac
`
  )
  writeStub(
    path.join(binDir, 'usermod'),
    `
echo "$*" >> "$TEST_LOG_DIR/usermod.log"
`
  )

  if (settings.sudo) {
    writeStub(
      path.join(binDir, 'sudo'),
      `
echo "$*" >> "$TEST_LOG_DIR/sudo.log"
exec "$@"
`
    )
  }
  if (settings.pkexec) {
    writeStub(
      path.join(binDir, 'pkexec'),
      `
echo "$*" >> "$TEST_LOG_DIR/pkexec.log"
exec "$@"
`
    )
  }
  if (settings.cargo) {
    writeStub(
      path.join(binDir, 'cargo'),
      `
echo "$*" >> "$TEST_LOG_DIR/cargo.log"
mkdir -p target/release
echo "locally built daemon" > target/release/omakeys-daemon
`
    )
  }

  const env = {
    PATH: binDir,
    HOME: base,
    PS4: '+L${LINENO}: ',
    TEST_LOG_DIR: logDir,
    TEST_ASSET_FILE: assetFile,
    TEST_ATTEST_FILE: attestationFile,
    TEST_ARCH: settings.arch,
    TEST_GROUPS: settings.groups,
    TEST_DAEMON_GROUP: settings.daemonGroup,
    TEST_REPO: REPO,
    CURL_ASSET_MODE: settings.assetCurl,
    CURL_ATTEST_MODE: settings.attestationsCurl,
    CURL_LATEST_MODE: settings.latestCurl,
  }
  if (settings.version !== null) env.OMAKEYS_VERSION = settings.version

  return {
    base,
    binDir,
    pluginDir,
    script,
    daemon,
    logDir,
    env,
    log(name) {
      const file = path.join(logDir, name)
      return fs.existsSync(file) ? fs.readFileSync(file, 'utf8') : null
    },
    run(args) {
      const result = spawnSync(BASH, ['-x', script, ...args], {
        cwd: pluginDir,
        encoding: 'utf8',
        env,
      })
      if (result.error) throw result.error
      const stderr = []
      for (const line of result.stderr.split('\n')) {
        const match = line.match(TRACE_PATTERN)
        if (match) tracedLines.add(Number(match[1]))
        else stderr.push(line)
      }
      return { status: result.status, stdout: result.stdout, stderr: stderr.join('\n') }
    },
  }
}

function assertInstallRefused(sandbox, pattern) {
  const result = sandbox.run(['--install'])
  assert.equal(result.status, 1, result.stderr)
  assert.match(result.stderr, pattern)
  assert.equal(fs.existsSync(sandbox.daemon), false)
  assert.equal(sandbox.log('install.log'), null)
}

test('bin/omarchy-install-omakeys', async (t) => {
  await t.test('installs the release daemon when the attestation verifies', { skip: skipReason }, () => {
    const sandbox = createSandbox()
    const result = sandbox.run(['--install'])

    assert.equal(result.status, 0, result.stderr)
    assert.match(result.stderr, /Build attestation cryptographically verified/)
    assert.match(result.stderr, new RegExp(`signer ${REPO}/${SIGNER_WORKFLOW}`))
    assert.equal(fs.statSync(sandbox.daemon).mode & 0o7777, 0o2755)
    assert.match(sandbox.log('install.log'), /-m 2755 .* input/)
    assert.match(sandbox.log('curl.log'), /attestations\/sha256:[0-9a-f]{64}/)
  })

  await t.test('verifies bundles signed with the colon separated DSSE encoding', { skip: skipReason }, () => {
    const sandbox = createSandbox({ pae: 'colon' })
    const result = sandbox.run(['--install'])

    assert.equal(result.status, 0, result.stderr)
    assert.equal(fs.statSync(sandbox.daemon).mode & 0o7777, 0o2755)
  })

  await t.test('refuses to install without openssl', { skip: skipReason }, () => {
    assertInstallRefused(createSandbox({ openssl: 'absent' }), /openssl is required to verify the build attestation/)
  })

  await t.test('refuses to install when the pinned CA chain is missing', { skip: skipReason }, () => {
    assertInstallRefused(createSandbox({ chain: 'missing' }), /pinned Sigstore CA chain not found/)
  })

  await t.test('refuses certificates that do not chain to the pinned CA', { skip: skipReason }, () => {
    assertInstallRefused(createSandbox({ chain: 'foreign' }), /does not chain to the pinned Sigstore CA/)
  })

  await t.test('does not trust the system CA store during chain verification', { skip: skipReason }, (t2) => {
    if (!SYSTEM_ROOT_CERTIFICATE) return t2.skip('no system root certificate available')
    assertInstallRefused(createSandbox({ certificate: 'systemTrusted' }), /does not chain to the pinned Sigstore CA/)
  })

  await t.test('refuses certificates dated outside their validity window', { skip: skipReason }, () => {
    assertInstallRefused(createSandbox({ integratedTime: 'stale' }), /does not chain to the pinned Sigstore CA/)
  })

  await t.test('refuses bundles without a transparency-log timestamp', { skip: skipReason }, () => {
    assertInstallRefused(createSandbox({ integratedTime: 'missing' }), /no usable transparency-log timestamp/)
  })

  await t.test('refuses bundles with a malformed transparency-log timestamp', { skip: skipReason }, () => {
    assertInstallRefused(createSandbox({ integratedTime: 'malformed' }), /no usable transparency-log timestamp/)
  })

  await t.test('refuses payloads that are not valid base64', { skip: skipReason }, () => {
    assertInstallRefused(createSandbox({ payloadEncoding: 'invalid' }), /attestation payload is not valid base64/)
  })

  await t.test('refuses signatures that are not valid base64', { skip: skipReason }, () => {
    assertInstallRefused(createSandbox({ signatureEncoding: 'invalid' }), /attestation signature is not valid base64/)
  })

  await t.test('refuses certificates that are not valid base64', { skip: skipReason }, () => {
    assertInstallRefused(createSandbox({ certificateEncoding: 'invalid' }), /attestation certificate is not valid base64/)
  })

  await t.test('refuses certificates that are not valid DER', { skip: skipReason }, () => {
    assertInstallRefused(createSandbox({ certificateEncoding: 'garbage' }), /attestation certificate could not be parsed/)
  })

  await t.test('refuses bundles without a DSSE payload type', { skip: skipReason }, () => {
    assertInstallRefused(createSandbox({ payloadType: 'missing' }), /carries no DSSE payload type/)
  })

  await t.test('refuses certificates issued for another signer identity', { skip: skipReason }, () => {
    assertInstallRefused(createSandbox({ certificate: 'foreignIdentity' }), /attestation was not signed by/)
  })

  await t.test('refuses certificates without the GitHub Actions OIDC issuer', { skip: skipReason }, () => {
    assertInstallRefused(createSandbox({ certificate: 'withoutIssuer' }), /carries no GitHub Actions OIDC issuer/)
  })

  await t.test('refuses bundles whose DSSE signature does not verify', { skip: skipReason }, () => {
    assertInstallRefused(createSandbox({ signature: 'tampered' }), /DSSE signature of the attestation payload does not verify/)
  })

  await t.test('refuses when the signed provenance names an unaccepted commit', { skip: skipReason }, () => {
    assertInstallRefused(createSandbox({ statement: { commit: KNOWN_COMMIT } }), /provenance does not match the pinned source commit/)
  })

  await t.test('refuses when the signed provenance predicate type differs', { skip: skipReason }, () => {
    assertInstallRefused(createSandbox({ statement: { predicateType: 'https://example.dev/provenance/v0' } }), /provenance does not match the pinned source commit/)
  })

  await t.test('refuses when the signed provenance subject digest differs', { skip: skipReason }, () => {
    assertInstallRefused(createSandbox({ statement: { subjectDigest: 'b'.repeat(64) } }), /provenance does not match the pinned source commit/)
  })

  await t.test('refuses when the API returns no attestation bundle', { skip: skipReason }, () => {
    assertInstallRefused(createSandbox({ attestationsCurl: 'empty' }), /no attestation bundle published/)
  })

  await t.test('refuses when the attestation API request fails', { skip: skipReason }, () => {
    assertInstallRefused(createSandbox({ attestationsCurl: 'fail' }), /refusing to install/)
  })

  await t.test('refuses when the downloaded asset does not match the pinned digest', { skip: skipReason }, () => {
    assertInstallRefused(createSandbox({ pin: 'mismatch' }), /checksum mismatch/)
  })

  await t.test('refuses when the pinned digest file is missing', { skip: skipReason }, () => {
    assertInstallRefused(createSandbox({ pin: 'missing' }), /supply-chain pin missing/)
  })

  await t.test('refuses when the pinned digest file is empty', { skip: skipReason }, () => {
    assertInstallRefused(createSandbox({ pin: 'empty' }), /digest file .* is empty/)
  })

  await t.test('refuses when the release asset download fails', { skip: skipReason }, () => {
    assertInstallRefused(createSandbox({ assetCurl: 'fail' }), /could not download/)
  })

  await t.test('refuses prebuilt installs on unsupported architectures', { skip: skipReason }, () => {
    const sandbox = createSandbox({ arch: 'aarch64' })
    const result = sandbox.run(['--install'])

    assert.equal(result.status, 1)
    assert.match(result.stderr, /no prebuilt daemon for aarch64/)
    assert.equal(sandbox.log('install.log'), null)
  })

  await t.test('fails when neither sudo nor pkexec is available', { skip: skipReason }, () => {
    const sandbox = createSandbox({ sudo: false, pkexec: false })
    const result = sandbox.run(['--install'])

    assert.equal(result.status, 1)
    assert.match(result.stderr, /need root/)
    assert.equal(sandbox.log('install.log'), null)
  })

  await t.test('falls back to pkexec for the privileged install', { skip: skipReason }, () => {
    const sandbox = createSandbox({ sudo: false, pkexec: true })
    const result = sandbox.run(['--install'])

    assert.equal(result.status, 0, result.stderr)
    assert.match(sandbox.log('pkexec.log'), /install -m 2755/)
    assert.equal(fs.statSync(sandbox.daemon).mode & 0o7777, 0o2755)
  })

  await t.test('resolves the version from the release redirect', { skip: skipReason }, () => {
    const sandbox = createSandbox({ version: null, manifest: false })
    const result = sandbox.run(['--install'])

    assert.equal(result.status, 0, result.stderr)
    assert.match(sandbox.log('curl.log'), /releases\/latest/)
    assert.match(sandbox.log('curl.log'), /releases\/download\/v1\.0\.0/)
  })

  await t.test('refuses when the version cannot be resolved', { skip: skipReason }, () => {
    const sandbox = createSandbox({ version: null, manifest: false, latestCurl: 'empty' })
    const result = sandbox.run(['--install'])

    assert.equal(result.status, 1)
    assert.match(result.stderr, /could not resolve version/)
  })

  await t.test('builds the daemon locally without sudo', { skip: skipReason }, () => {
    const sandbox = createSandbox({ cargo: true })
    const result = sandbox.run(['--build'])

    assert.equal(result.status, 0, result.stderr)
    assert.equal(sandbox.log('sudo.log'), null)
    assert.equal(fs.statSync(sandbox.daemon).mode & 0o7777, 0o755)
    assert.equal(fs.readFileSync(sandbox.daemon, 'utf8'), 'locally built daemon\n')
  })

  await t.test('builds and installs the daemon setgid', { skip: skipReason }, () => {
    const sandbox = createSandbox({ cargo: true })
    const result = sandbox.run(['--build-install'])

    assert.equal(result.status, 0, result.stderr)
    assert.match(sandbox.log('install.log'), /-m 2755 .* input/)
    assert.equal(fs.statSync(sandbox.daemon).mode & 0o7777, 0o2755)
  })

  await t.test('refuses to build without cargo', { skip: skipReason }, () => {
    const sandbox = createSandbox({ cargo: false })
    const result = sandbox.run(['--build'])

    assert.equal(result.status, 1)
    assert.match(result.stderr, /cargo not found/)
  })

  await t.test('reports daemon status', { skip: skipReason }, () => {
    const missing = createSandbox()
    const absent = missing.run(['--check'])
    assert.equal(absent.status, 0)
    assert.match(absent.stdout, /daemon: NOT installed/)

    const installed = createSandbox({ daemon: 0o755 })
    const present = installed.run(['--check'])
    assert.equal(present.status, 0)
    assert.match(present.stdout, /daemon: present \(123 bytes\)/)
    assert.match(present.stdout, /setgid: NOT active/)

    const active = createSandbox({ daemon: 0o2755, daemonGroup: 'input' })
    const running = active.run(['--check'])
    assert.equal(running.status, 0)
    assert.match(running.stdout, /setgid: active \(group input/)
  })

  await t.test('uninstalls the daemon', { skip: skipReason }, () => {
    const absent = createSandbox()
    const missing = absent.run(['--uninstall'])
    assert.equal(missing.status, 0)
    assert.match(missing.stdout, /nothing to remove/)

    const installed = createSandbox({ daemon: 0o2755 })
    const removed = installed.run(['--uninstall'])
    assert.equal(removed.status, 0)
    assert.equal(fs.existsSync(installed.daemon), false)
  })

  await t.test('adds the user to the input group only after verification', { skip: skipReason }, () => {
    const verified = createSandbox()
    const ok = verified.run(['--usermod'])
    assert.equal(ok.status, 0, ok.stderr)
    assert.match(verified.log('usermod.log'), /-aG input tester/)

    const refused = createSandbox({ signature: 'tampered' })
    const failed = refused.run(['--usermod'])
    assert.equal(failed.status, 1)
    assert.equal(refused.log('usermod.log'), null)
    assert.equal(fs.existsSync(refused.daemon), false)

    const member = createSandbox({ groups: 'sys input video' })
    const already = member.run(['--usermod'])
    assert.equal(already.status, 0, already.stderr)
    assert.match(already.stdout, /already in the input group/)
  })

  await t.test('prints usage for unknown or missing flags', { skip: skipReason }, () => {
    const sandbox = createSandbox()
    for (const args of [[], ['--nope'], ['-h']]) {
      const result = sandbox.run(args)
      assert.equal(result.status, 1)
      assert.match(result.stdout + result.stderr, /Usage: omarchy-install-omakeys/)
    }
  })
})

test('pinned Sigstore trust anchor', async (t) => {
  const skip = OPENSSL ? false : 'openssl is not available'
  const fixture = JSON.parse(fs.readFileSync(FIXTURE_PATH, 'utf8'))
  const directory = fs.mkdtempSync(path.join(tempRoot, 'trust-anchor-'))
  const emptyCaDir = path.join(directory, 'empty-ca')
  fs.mkdirSync(emptyCaDir)
  const certificatePath = path.join(directory, 'certificate.der')
  const certificatePem = path.join(directory, 'certificate.pem')
  const publicKeyPath = path.join(directory, 'public-key.pem')
  const payloadPath = path.join(directory, 'payload.bin')
  const signaturePath = path.join(directory, 'signature.bin')
  const messagePath = path.join(directory, 'dsse-message.bin')
  const payload = Buffer.from(fixture.dsseEnvelope.payload, 'base64')
  const signature = Buffer.from(fixture.dsseEnvelope.signatures[0].sig, 'base64')
  const integratedTime = String(fixture.verificationMaterial.tlogEntries[0].integratedTime)
  const statement = JSON.parse(payload.toString('utf8'))

  fs.writeFileSync(certificatePath, Buffer.from(fixture.verificationMaterial.certificate.rawBytes, 'base64'))
  fs.writeFileSync(payloadPath, payload)
  fs.writeFileSync(signaturePath, signature)
  fs.writeFileSync(messagePath, dsseMessage(fixture.dsseEnvelope.payloadType, payload.toString('utf8'), 'space'))

  await t.test('chains the published v1.0.0 attestation to the pinned CA', { skip }, () => {
    runOpenssl(['x509', '-inform', 'der', '-in', certificatePath, '-out', certificatePem], directory)
    const verified = runOpenssl(
      ['verify', '-attime', integratedTime, '-CAfile', CHAIN_PATH, '-CApath', emptyCaDir, certificatePem],
      directory
    )
    assert.match(verified.stdout, /: OK/)
  })

  await t.test('verifies the DSSE signature of the published bundle', { skip }, () => {
    runOpenssl(['x509', '-in', certificatePem, '-pubkey', '-noout', '-out', publicKeyPath], directory)
    const verified = runOpenssl(
      ['dgst', '-sha256', '-verify', publicKeyPath, '-signature', signaturePath, messagePath],
      directory
    )
    assert.match(verified.stdout, /Verified OK/)
  })

  await t.test('binds the published bundle to this repository and workflow', { skip }, () => {
    const certificateText = runOpenssl(['x509', '-in', certificatePem, '-noout', '-text'], directory).stdout
    assert.ok(certificateText.includes(`URI:https://github.com/${REPO}/${SIGNER_WORKFLOW}@`))
    assert.ok(certificateText.includes(OIDC_ISSUER))
    assert.equal(statement.subject[0].digest.sha256, fs.readFileSync(PIN_PATH, 'utf8').trim().split(/\s+/)[0])
    assert.ok(ACCEPTED_COMMITS.includes(statement.predicate.buildDefinition.resolvedDependencies[0].digest.gitCommit))
  })

  await t.test('pins only Sigstore public-good CA certificates', { skip }, () => {
    const pem = fs.readFileSync(CHAIN_PATH, 'utf8')
    const subjects = []
    for (const block of pem.split('-----BEGIN CERTIFICATE-----').slice(1)) {
      const certificate = path.join(directory, `chain-${subjects.length}.pem`)
      fs.writeFileSync(certificate, `-----BEGIN CERTIFICATE-----${block}`)
      subjects.push(runOpenssl(['x509', '-in', certificate, '-noout', '-subject'], directory).stdout)
    }
    assert.equal(subjects.length, 3)
    for (const subject of subjects) assert.match(subject, /O=sigstore\.dev/)
  })

  await t.test('keeps the system CA store out of the trust path', { skip }, (t2) => {
    const systemRoot = SYSTEM_ROOT_CERTIFICATE
    if (!systemRoot) return t2.skip('no system root certificate available')
    const verified = spawnSync(
      OPENSSL,
      ['verify', '-CAfile', CHAIN_PATH, '-CApath', emptyCaDir, systemRoot],
      { encoding: 'utf8' }
    )
    assert.notEqual(verified.status, 0, `unexpectedly trusted: ${verified.stdout}`)
  })
})
