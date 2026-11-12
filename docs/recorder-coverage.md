# Recorder coverage

What the vanilla harness covers, as of 2026-11-06. This is a statement of evidence at hand, and it changes with each
recording pass. `generated/conformance-evidence.json` is what a claim rests on. A row here says a recording finished, not
that it matches.

## Adapters and recordings

All 50 registered releases have an adapter in the recorder's `mod/mc`, and all 50 were recorded.

The table is the last completed full pass: 2026-11-06, 01:46 to 11:14 UTC by the `recordedAt` of each `index.json`.
1.8.8 to 1.19.2 ran on the remote host, 1.19.3 to 26.3 on this machine. The counts are those of `todo.md`'s note on that
pass. The rows of 1.19.3 to 26.3 were checked against their `index.json` files on 2026-11-06. The older rows were not: a new
pass is rewriting that folder on the remote host, and the older indexes of this pass are not on this machine.
`scripts/release-status-doc.cjs` reads this table.

| Releases | Scenarios in the index | Not `ok` | Where |
| --- | --- | --- | --- |
| 1.8.8 | 1,866 | none | remote |
| 1.9.4 | 1,992 | 1 invalid | remote |
| 1.10.2 | 2,028 | 1 invalid | remote |
| 1.11.2 | 2,029 | 3 invalid, 5 error | remote |
| 1.12.2 | 2,034 | 2 invalid, 5 error | remote |
| 1.13.2 | 2,226 | none | remote |
| 1.14.4 | 2,388 | none | remote |
| 1.15 to 1.15.2 | 2,446 | none | remote |
| 1.16, 1.16.1 | 2,502 | none | remote |
| 1.16.2 to 1.16.5 | 2,505 | none | remote |
| 1.17 to 1.18.2 | 2,575 | none | remote |
| 1.19 to 1.19.2 | 2,613 | none | remote |
| 1.19.3 | 2,616 | 3 skipped | here |
| 1.19.4 | 2,618 | 3 skipped | here |
| 1.20 | 2,638 | 1 error | here |
| 1.20.1, 1.20.2 | 2,638 | none | here |
| 1.20.3, 1.20.4 | 2,647 | none | here |
| 1.20.5, 1.20.6 | 2,781 | 3 error, 1 skipped each | here |
| 1.21 to 1.21.5 | 2,812 | none | here |
| 1.21.6 to 1.21.8 | 2,825 | none | here |
| 1.21.9, 1.21.10 | 2,829 | none | here |
| 1.21.11 to 26.1.2 | 2,855 | none | here |
| 26.2 | 2,859 | none | here |
| 26.3 | 2,866 | none | here |

The counts differ because a scenario exists only where its feature does, so a release's count is its own corpus.

Not recorded, on the rows read here:

- 1.19.3 and 1.19.4 skipped `physrec/v_camel_dash`, `physrec/v_camel_sprint` and `physrec/v_camel_walk`.
- 1.20 ended `physrec/x_far29m_sprint_jump` in an error.
- 1.20.5 and 1.20.6 ended `physrec/m_c_mount_click_donkey_sneak_inventory`, `physrec/m_c_open_donkey_riding` and
  `physrec/m_c_open_mule_riding` in an error, and skipped `physrec/pj_arrow_breeze_deflects`.

These recordings are not current. They predate the pass triage's mod changes, which reach every release, and the scenarios
added since: at commit `0eaaa0d` the scenarios compile to 3,322 on 1.20.4, against 2,647 in this pass. The replay of this
pass is not recorded in `todo.md`. A final pass is running; it has no result here yet.

An adapter directory is not evidence. The recorder's `mod/check-mixins.py` lists members a release lacks, and has not been shown
to run over all of them.

## What cannot pass

| Case | Result | Where |
| --- | --- | --- |
| No recording matches the selection | exit 1, `no recordings match` | the recorder's `replay/cli.ts` |
| Nothing compared | `untested`, and a selection of those fails | the recorder's `replay/verdict.ts` |
| A misfiled or short recording, or a snapshot missing a field of its release | `BAD`, fail | the recorder's `replay/validate.ts` |
| A recording of another scenario hash or stale compile | refused | `recorded from another version of its scenario` |
| A shard reporting another selection | a run failure | the recorder's `runner/outcomes.ts` |
| A vehicle the engine has no model for | `unsupported`, not a pass | the recorder's `replay/verdict.ts` |

## Identity

A recording keeps what it was made from: `header.provenance`, and beside the index `builds/<id>.json`, holds the
game and library digests, the mod, its sources, the launch, the schema, the compiled scenario index and its sources,
the codec, the JDK, and the commit with a digest of any changes. A pin in `conformance-evidence.json` cites the hash
of `index.json` and its build id, with the replay's own engine and addon identity.

## Plan for the rest

B219 owns validating the adapters from a clean pass, and B224 and B235 own the corpus and harness requirements per release.
