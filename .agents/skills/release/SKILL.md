---
name: release
description: Prepare or publish a Kver release using the project runbook.
disable-model-invocation: true
---

# Release Kver

Follow the repository's `docs/releasing.md` from version selection through
verification. If no version was supplied, recommend one and wait for confirmation.
Preparation does not authorize publication. An explicit request to publish the
specified version covers the release steps, including the Arch recipe update.

For publication, finish the runbook or report which outputs are live, what
remains blocked, and how to resume without retagging.
