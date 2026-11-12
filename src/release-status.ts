// What each registered release has shown (scripts/release-admission.cjs): registering a release lets it run and says
// nothing of parity. `requireParity` refuses a release that is not admitted, naming each dimension that keeps it out.
import status from './release-status.json';

export interface ReleaseDimension {
    readonly status: string;
    readonly reasons: readonly string[];
}

export interface ReleaseStatus {
    readonly source: ReleaseDimension;
    readonly data: ReleaseDimension;
    readonly implementation: ReleaseDimension;
    /** Whether every dimension passed, conformance among them: only a pinned, current and complete trace does that. */
    readonly admitted: boolean;
}

const RELEASES: Readonly<Record<string, ReleaseStatus>> = status.releases;

/** What a release has shown, undefined for one that is not registered. */
export function releaseStatus(version: string): ReleaseStatus | undefined {
    return Object.prototype.hasOwnProperty.call(RELEASES, version) ? RELEASES[version] : undefined;
}

/** Throws unless the release is admitted, naming the evidence each dimension lacks. */
export function assertReleaseAdmitted(version: string): void {
    const found = releaseStatus(version);
    if (!found) throw new Error(`${version} is not a registered release`);
    if (found.admitted) return;
    const lacking = ([['source', found.source, ['exact', 'alias']], ['data', found.data, ['complete']], ['implementation', found.implementation, ['complete']]] as const)
        .filter(([, dimension, passing]) => !(passing as readonly string[]).includes(dimension.status))
        .map(([name, dimension]) => `${name} ${dimension.status}${dimension.reasons.length > 0 ? ` (${dimension.reasons.join('; ')})` : ''}`);
    throw new Error(`${version} is not admitted for parity: ${[...lacking, 'conformance unverified (see scripts/release-admission.cjs)'].join(', ')}`);
}
