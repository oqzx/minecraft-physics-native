import { isSupportedVersion, type SupportedVersion } from '../versions';

/**
 * Resolve one exact release before touching a binding or creating a world. The caller's registry declaration is
 * the compatibility boundary: a shared protocol number or minor release does not establish equivalent data.
 */
export function resolveRegistryRelease(registry: unknown, requested?: unknown): SupportedVersion {
    const identity = registry !== null && typeof registry === 'object'
        ? (registry as { version?: { minecraftVersion?: unknown; '=='?: unknown } | null }).version
        : undefined;
    const declared = identity?.minecraftVersion;
    if (typeof declared !== 'string' || declared.length === 0) {
        const target = typeof requested === 'string' ? ` to serve Minecraft ${requested}` : '';
        throw new Error(`registry must declare an exact Minecraft release in version.minecraftVersion${target}`);
    }
    const selected = requested === undefined ? declared : requested;
    if (typeof selected !== 'string' || !isSupportedVersion(selected)) {
        throw new Error(`unsupported decompiled Minecraft version: ${String(selected)}`);
    }
    if (declared !== selected) {
        throw new Error(`registry release ${declared} cannot serve Minecraft ${selected}: supply a registry declaring the selected exact release`);
    }
    // Some data providers retain the requested release in their comparison callbacks while exposing a fallback
    // release in minecraftVersion. Equality can reject that contradiction, but never approve a different release.
    const equals = identity?.['=='];
    if (equals !== undefined) {
        let consistent = false;
        try {
            consistent = typeof equals === 'function' && equals.call(identity, declared) === true;
        } catch {
            // A provider unable to compare its own declaration cannot establish the registry's release identity.
        }
        if (!consistent) {
            throw new Error(`registry release ${declared} has inconsistent version metadata: version['=='] cannot confirm its declaration; supply an exact-release registry`);
        }
    }
    return selected;
}
