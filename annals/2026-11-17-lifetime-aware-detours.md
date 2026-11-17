# Lifetime-aware named detours

- Date: 2026-11-17
- Status: accepted; implementation uncommitted
- Scope: retour-rs typed detour API and native borrow contracts
- Repository: https://github.com/Chiv2-Community/retour-rs.git
- Branch: `feature/lifetime-detours`
- Recording base: `3209cd0aa9c339fdca073ef2d00e46c0f0116bd4`; no exact or nearest reachable tag found.
- Implementation: uncommitted working-tree changes at the recording base; no implementation commit or tag.
- Source: supplied compacted checkpoint of session `ses_e1dce76ceffelYhFmWfsMKIFnP`, through response `msg_1eb15939b001SeSmSiG7tX5AOZ`. Earlier conversation is not complete. Authoring agent: Annalist. Participants and related issue or PR: unknown.

## Decision and context

Add `generic_detour!` as a separate, named, `RawDetour`-backed interface for higher-ranked function signatures. Its generated `call` preserves per-call lifetime relationships, including borrowed results. Keep `Function`, `GenericDetour`, and `StaticDetour` interfaces unchanged. The existing `Function` trait has fixed `Arguments` and `Output` associated types, whereas the new interface retains the `for<...>` signature rather than decomposing it into those types. This rationale is supported by the checkpoint and the implementation ([trait](../src/traits.rs), [macro](../src/generic_macros.rs)); detailed earlier deliberation is unavailable.

Construction and patching remain unsafe. Calls follow the declared function's safety, and callers must ensure native reference validity, ABI, aliasing, lifetimes, and synchronization. The API cannot validate native objects or extend their lifetimes; use raw pointers when reference guarantees cannot be established. Capturing closures are not supported.

## Options and consequences

- Chosen: an additive macro preserves existing interfaces and expresses input/output lifetime relationships. It requires named declarations and at least one explicit lifetime, and remains subject to raw patching safety constraints.
- Changing the existing trait or detour interfaces was not chosen; compatibility is preserved. The available checkpoint does not establish whether a specific redesign was evaluated or rejected, or its detailed tradeoffs.

The implementation is not yet committed, released, or independently verified here by rerunning tests. The uncommitted [fixtures](../tests/lifetimes.rs) cover shared and mutable receivers, independent and nested lifetimes, and patching; [macro documentation](../src/generic_macros.rs) includes compile-fail examples for invalid lifetime relationships. Available local debug, release, `42-args`, and Windows/Wine logs report passing results, including six Windows fixtures; their exact commands and correspondence to the final working-tree contents were not independently verified. A local static-feature log reports passing library/integration results. The checkpoint reports that a full nightly `static-detour` run failed in the unchanged `cat_detour` example on an invalid runtime `open` symbol; that failure was not independently reproduced.

## Related work and follow-up

The source checkpoint and a local migration checkpoint say the separate UnchainedPlugin call-only binding migration was paused when this work started. Its requested organization is one native class per file under UE module namespaces; the latest migration was incomplete and had not passed a build. Its earlier 498-test result does not validate the paused state. This is context, not evidence that the plugin migration was completed or that its files belong in this repository.

Before release, commit and review the implementation, verify the relevant test commands against the committed tree, investigate or separately track the nightly example failure, and resume plugin validation independently. Reconsider the API if higher-ranked signatures cannot be expressed safely or compatibly, supported ABIs fail, or native callers cannot establish the documented reference guarantees. Missing history includes exact option deliberations, source participant identities, and a durable issue or PR linking the two workstreams.

## Subsequent developments

2026-11-17: The `generic_detour!` name in the decision summary is superseded by the agreement to use `lifetime_detour!`; the rename remains unimplemented. The design decision remains accepted. See [Lifetime detour macro naming](2026-11-17-lifetime-detour-naming.md).
