# Lifetime detour macro naming

- Date: 2026-11-17
- Status: accepted; rename unimplemented
- Scope: lifetime-aware detour macro public name
- Repository: https://github.com/Chiv2-Community/retour-rs.git
- Branch: `feature/lifetime-detours`
- Recording base: `5c4fbdae0f46be21f2a7f955ba213df66a4d833f`; no exact or nearest reachable tag found.
- Implementation: uncommitted working tree, still named `generic_detour!`; no rename commit or tag.
- Source: supplied compacted checkpoint of session `ses_e1dce76ceffelYhFmWfsMKIFnP` through response `msg_1eb16f1be001I4OTRdpwrbhvA0`; authoring agent: Annalist. Earlier conversation is incomplete; participant identities and related issue or PR are unknown.

## Decision and context

Name the new higher-ranked detour declaration `lifetime_detour!` to make its lifetime feature explicit. The user wanted the feature reflected in the name; the assistant agreed that `lifetime_detour!` was clearer than `typed_detour!`. This is a naming agreement, not evidence of an implemented rename. The [prototype](../src/generic_macros.rs) still declares `generic_detour!` in the uncommitted working tree. The [earlier design record](2026-11-17-lifetime-aware-detours.md) remains applicable, but its chosen macro name is superseded by this agreement.

## Options and rationale

- `lifetime_detour!`: chosen for explicit identification of the lifetime feature; rationale is attributed to the supplied checkpoint, not independently reconstructed.
- `typed_detour!`: considered less clear about that feature.
- `scoped_detour!`: considered misleading because it suggests scoped hook activation.
- `borrowed_detour!`: considered misleading because it could suggest captured borrowed state.
- `generic_detour!`: prototype name; the checkpoint does not establish further rationale for its original selection.

## Consequences and follow-up

The agreement does not change the API's safety constraints, HRTB design, or lack of captured closure support. Rename the declaration and its uses and documentation before treating the agreed name as implemented; verify compatibility and tests against the resulting tree. Existing local debug and Windows/Wine logs report passing tests, but their relationship to the present uncommitted files and a future rename has not been established. The earlier record describes the reported nightly example failure and paused plugin migration.

Reconsider the name if API review finds that it misstates the feature or creates a compatibility problem. The full source conversation, precise rename schedule, compatibility policy for the prototype name, and durable issue or PR are unavailable; no alias or release is implied.
