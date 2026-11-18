import type { RoomRecoveryInfo } from './types';
export function recoverySignature(recovery: RoomRecoveryInfo): string {
  const backup = recovery.backup;
  return JSON.stringify([recovery.id, recovery.available, recovery.primaryError, recovery.backupError,
    backup?.id, backup?.title, backup?.updatedAt, backup?.entryCount]);
}
export function canConfirmRecovery(recovery: RoomRecoveryInfo, confirmation: string | null, locked: boolean): boolean {
  return !locked && recovery.available && recovery.backup !== null && recovery.backup.id === recovery.id
    && confirmation !== null && confirmation === recoverySignature(recovery);
}
