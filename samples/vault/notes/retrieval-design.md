---
title: Retrieval Design
tags: [design, retrieval, okf]
---

# Retrieval Design

Deterministic retrieval uses the structure that already exists in a vault.

## Why not embeddings

Vector search hides *why* a note matched. Tags, wikilinks and headings are
explicit, so a result can explain itself and can be reproduced byte for byte.

## Token budget

Whole notes are verbose. Truncate the body to a character budget and keep the
metadata header, so the model still knows where the text came from.

## Open Knowledge Format

The Open Knowledge Format favours markdown files that describe themselves with
frontmatter. A vault is already close to this shape, so no conversion step is
needed. #okf #markdown

See [[index]] and [[alpha]].
