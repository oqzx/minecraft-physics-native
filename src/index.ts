import type { Bot } from 'mineflayer';
import { LoadNativePhysics } from './native';
import type { NativePhysicsBinding } from './native';

declare module 'mineflayer' {
    interface Bot {
        NativePhysics: NativePhysicsBinding;
    }
}

export default function loader(bot: Bot): void {
    if (!bot.NativePhysics) {
        bot.NativePhysics = LoadNativePhysics();
    }
}

export {
    LoadNativePhysics,
    NATIVE_PLAYER_ENTITY_LENGTH,
    NATIVE_PLAYER_LAYOUT,
    NativePlayerEntityFlag,
    NativePlayerInput,
    NativePlayerInputFlag,
    NativePlayerOutput,
    NativePlayerOutputFlag,
    nativeBackend,
} from './native';
export type * from './native';
export { DEFAULT_CHUNK_TICKS, runBulk } from './bulk';
export type { BulkRunOptions, BulkRunOutcome } from './bulk';
export { CHECKPOINT_SCHEMA, decodeCheckpoint, encodeCheckpoint } from './checkpoint';
export type { NativeCheckpointKind, NativeCheckpointParts } from './checkpoint';
export * from './mineflayer';
export { SUPPORTED_VERSIONS, isSupportedVersion } from './versions';
export { assertReleaseAdmitted, releaseStatus } from './release-status';
export type { ReleaseDimension, ReleaseStatus } from './release-status';
export type { SupportedVersion } from './versions';
