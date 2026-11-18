import test from 'node:test';
import assert from 'node:assert/strict';
import { recoverySignature, canConfirmRecovery } from './room-recovery.ts';

function recovery() { return { id: 'room', available: true, primaryError: 'broken primary.json', backupError: null, backup: { id: 'room', title: 'Previous', updatedAt: '2026-11-17T01:00:00Z', entryCount: 12 } }; }
test('recovery requires an explicit confirmation bound to the displayed backup', () => {
  const info = recovery();
  assert.equal(canConfirmRecovery(info, null, false), false);
  const confirmation = recoverySignature(info);
  assert.equal(canConfirmRecovery(info, confirmation, false), true);
  assert.equal(canConfirmRecovery({ ...info, backup: { ...info.backup, updatedAt: 'later' } }, confirmation, false), false);
  assert.equal(canConfirmRecovery(info, confirmation, true), false);
});
test('missing, unavailable or mismatched backups cannot be restored', () => {
  const info = recovery();
  for (const changed of [{ ...info, available: false }, { ...info, backup: null }, { ...info, backup: { ...info.backup, id: 'other' } }]) assert.equal(canConfirmRecovery(changed, recoverySignature(changed), false), false);
});
