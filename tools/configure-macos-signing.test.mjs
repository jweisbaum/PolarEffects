import assert from 'node:assert/strict';
import { test } from 'node:test';
import { signingEnvironment } from './configure-macos-signing.mjs';

const certificate = { APPLE_CERTIFICATE: 'fixture', APPLE_SIGNING_IDENTITY: 'Developer ID Application: Fixture' };
const notarization = { APPLE_ID: 'fixture@example.invalid', APPLE_PASSWORD: 'fixture', APPLE_TEAM_ID: 'fixture' };

test('missing and empty secrets produce only the ad-hoc identity', () => {
  assert.deepEqual(signingEnvironment({}), { APPLE_SIGNING_IDENTITY: '-' });
  assert.deepEqual(signingEnvironment(Object.fromEntries(
    [...Object.keys(certificate), ...Object.keys(notarization), 'APPLE_CERTIFICATE_PASSWORD'].map(key => [key, ''])
  )), { APPLE_SIGNING_IDENTITY: '-' });
});

test('signed builds accept an empty certificate password and complete notarization', () => {
  assert.deepEqual(signingEnvironment(certificate), { ...certificate, APPLE_CERTIFICATE_PASSWORD: '' });
  const complete = { ...certificate, ...notarization, APPLE_CERTIFICATE_PASSWORD: 'fixture' };
  assert.deepEqual(signingEnvironment(complete), complete);
});

test('each incomplete notarization combination fails before a build', () => {
  const keys = Object.keys(notarization);
  for (let mask = 1; mask < 7; mask++) {
    const partial = Object.fromEntries(keys.filter((_, i) => mask & (1 << i)).map(key => [key, notarization[key]]));
    assert.throws(() => signingEnvironment({ ...certificate, ...partial }), /requires APPLE_ID/);
  }
});

test('orphaned credentials and a missing or ad-hoc certificate identity fail', () => {
  for (const env of [notarization, { APPLE_CERTIFICATE_PASSWORD: 'fixture' },
    { APPLE_SIGNING_IDENTITY: certificate.APPLE_SIGNING_IDENTITY }]) {
    assert.throws(() => signingEnvironment(env), /require APPLE_CERTIFICATE/);
  }
  for (const identity of ['', '-']) {
    assert.throws(() => signingEnvironment({ ...certificate, APPLE_SIGNING_IDENTITY: identity }), /requires a Developer ID/);
  }
});
