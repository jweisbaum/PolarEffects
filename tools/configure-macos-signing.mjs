// Absent secrets mean an ad-hoc build. Partial credentials must never silently
// produce an unsigned or unnotarized release someone expected to be signed.
import { appendFileSync } from 'node:fs';
import { randomUUID } from 'node:crypto';
import { pathToFileURL } from 'node:url';

export function signingEnvironment(env) {
  const certificate = env.APPLE_CERTIFICATE;
  const identity = env.APPLE_SIGNING_IDENTITY;
  const notarization = ['APPLE_ID', 'APPLE_PASSWORD', 'APPLE_TEAM_ID'];
  const present = notarization.filter(key => env[key]);
  if (present.length && present.length !== notarization.length) {
    throw new Error('Notarization requires APPLE_ID, APPLE_PASSWORD and APPLE_TEAM_ID together');
  }
  if (!certificate) {
    if (env.APPLE_CERTIFICATE_PASSWORD || (identity && identity !== '-') || present.length) {
      throw new Error('macOS signing credentials require APPLE_CERTIFICATE');
    }
    return { APPLE_SIGNING_IDENTITY: '-' };
  }
  if (!identity || identity === '-') {
    throw new Error('APPLE_CERTIFICATE requires a Developer ID APPLE_SIGNING_IDENTITY');
  }
  return {
    APPLE_CERTIFICATE: certificate,
    APPLE_CERTIFICATE_PASSWORD: env.APPLE_CERTIFICATE_PASSWORD || '',
    APPLE_SIGNING_IDENTITY: identity,
    ...Object.fromEntries(present.map(key => [key, env[key]])),
  };
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const signing = signingEnvironment(process.env);
  for (const [key, value] of Object.entries(signing)) {
    const delimiter = randomUUID();
    appendFileSync(process.env.GITHUB_ENV, `${key}<<${delimiter}\n${value}\n${delimiter}\n`);
  }
  console.log(signing.APPLE_ID ? 'Developer ID signing and notarization configured.'
    : signing.APPLE_CERTIFICATE ? 'Developer ID signing configured; notarization is not configured.'
      : 'No signing credentials: ad-hoc macOS build (not notarized).');
}
