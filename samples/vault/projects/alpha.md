---
title: Alpha
tags:
  - project
  - project/alpha
  - active
status: in-progress
---

# Alpha

First implementation of the retrieval server.

## Goal

Expose the four tools `search_by_tag`, `search_by_term`, `get_linked_notes` and
`get_note` over the Model Context Protocol.

## Open questions

- Should nested tags match their parents? Yes, `project` should match
  `project/alpha`.
- How deep should the graph walk go by default? One hop.

Depends on [[retrieval-design]]. Tracked in [[index]].
