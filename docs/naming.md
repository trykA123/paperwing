# Legacy names

Skein was previously named PaperWing. These identifiers keep the old name on purpose. Renaming them
would orphan data on existing installs or break the scripts and evidence that read them back.

## Kept identifiers

| Identifier | What it is | Why it keeps the old name |
| --- | --- | --- |
| `dev.paperwing.app` | Bundle identifier (`tauri.conf.json`) | Names the settings, cache and recovery folders under `%APPDATA%` and `%LOCALAPPDATA%`. |
| `dev.paperwing.testing` | Test-profile identifier (`tauri.test.conf.json`, `test_profile.rs`, scripts) | Names the folders inside existing disposable test profiles. |
| `paperwing` | Keyring service (`credentials.rs` `SERVICE`) | Stored tokens are keyed by this service name. |
| `paperwing-testing-fixtures-v1` | Keyring service for test-profile builds | Fixture tokens in existing test wallets use this name. |
| `.paperwing-<id>.lock`, `.paperwing-<id>.tmp`, `.paperwing-<id>.previous` | Windows file-edit keeper, staged file and backup (`files.rs`) | Recovery records point at these names after a crash. |
| `.paperwing-stage-` | Linux staged-write prefix (`linux_guard/mutation.rs`, `linux_journal`) | Recovery scans for this prefix and checks its exact length. |
| `.paperwing-reclone-<ts>.lock` | Windows reclone keeper lock (`clone.rs`) | Lock files written to disk keep their name. |
| `paperwing.commitMax` | `localStorage` key for the commit dialog width | Keeps the saved preference in existing webview profiles. |
| `.paperwing-disposable`, `paperwing-disposable-profile-v1`, `paperwing-disposable-fixture-v1` | Ownership marker for test profiles and fixtures | Existing profiles and fixtures carry this marker; scripts refuse folders without it. |
| `paperwing-test-profile-build-v1`, `--paperwing-test-profile-info` | Test-profile build marker and inspection flag | `native-profile.ts` and `linux-credentials.py` check built artifacts for them. |
| `--paperwing-credential-drill` | Credential drill flag of test-profile builds | `linux-credentials.py` calls existing artifacts with it. |
| `PaperWing private test` | KWallet wallet name in `linux-credentials.py` | The wallet may already exist in reused disposable profiles. |
| `.paperwing-{guard,process,diff,journal,parent,folder}-fixture`, `.paperwing-parent-compatibility-fixture`, `.paperwing-process-evidence`, `.paperwing-test-root`, and their `-v1` contents | Fixture ownership markers | Tests and scripts check them before reusing or deleting a fixture; evidence fixtures persist between runs. |
| `paperwing-parent-compatibility` | Schema of the frozen compatibility fixture manifest | Existing manifests carry this schema. |
| `.skillify/evidence/paperwing/` | Evidence and Windows VM folder | Existing evidence, fixtures and the VM live there. |
| `PAPERWING_*` environment variables | Legacy names of the `SKEIN_*` test variables | Read as a fallback when the `SKEIN_*` name is unset (`env_names.rs`, `benchmark.ts`). |
| `CHANGELOG.md` history, `plans/2026-*`, `.skillify/plans/2026-*` | Dated history | Records what happened under the old name. |
| `feat/paperwing-linux-completion`, `work/paperwing-08-prototype`, `.crew/paperwing-*`, `/mnt/Sabrent/homelab/paperwing-08-prototype` | Branch, worktree and folder names outside the tree | They exist under these names. |
| `docs/paperwing-tour.svg` | README tour animation | Its content still says PaperWing; it needs a regenerated tour, not a text edit. |

## Environment variables

Test and benchmark variables use the `SKEIN_` prefix, for example `SKEIN_TEST_TMP` and
`VITE_SKEIN_BENCHMARK`. The `PAPERWING_` name is still read when the `SKEIN_` name is unset.
