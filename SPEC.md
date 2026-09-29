# SPEC.md - Obsidian Notes MCP Server

## Project Overview
A command-line Model Context Protocol (MCP) server that retrieves relevant Obsidian markdown notes for LLM consumption. Rather than using vector-based RAG, the server leverages the inherent structure of Obsidian vaults—tags, wikilinks, backlinks, and explicit terms—to perform deterministic, graph-aware retrieval. This aligns with Open Knowledge Format principles by treating notes as interconnected, self-describing entities rather than opaque embedding chunks.

## Core Architecture & Components
- **Vault Indexer:** Scans a configured Obsidian vault directory, parses markdown files, and builds an in-memory index of notes, tags, wikilinks, backlinks, frontmatter, and headings.
- **Query Parser:** Interprets incoming MCP tool-call parameters (tags, search terms, linked-note references, and traversal depth) into a structured retrieval query.
- **Graph-Aware Retriever:** Resolves relevant notes by intersecting tag matches, term matches (title, heading, body), and link-graph proximity (direct links, backlinks, and N-hop neighbors). Ranks and deduplicates results.
- **MCP Interface Layer:** Exposes retrieval capabilities as MCP tools over stdio (and optionally HTTP), compatible with LLM clients such as Claude Desktop or other MCP hosts.
- **Output Formatter:** Serializes retrieved notes into clean, LLM-friendly payloads (markdown with metadata headers, or structured JSON) with optional token-budget truncation.

## Non-Goals
- Building a full graphical user interface (CLI and MCP stdio only).
- Implementing RAG, vector embeddings, or semantic similarity search.
- Supporting PDF, DOCX, or other non-markdown file ingestion.
- Writing back to, editing, or mutating the Obsidian vault (read-only access).
- Hosting a multi-user authenticated service (single-user, local-first).

## Acceptance Criteria
- Successfully indexes a sample Obsidian vault containing markdown notes with tags, wikilinks, and frontmatter.
- Returns relevant notes when queried by tag, by term, by linked-note reference, or by any combination thereof.
- Exposes at least the following MCP tools: `search_by_tag`, `search_by_term`, `get_linked_notes`, and `get_note`.
- Operates entirely locally via a Python command-line script managed in a virtual environment, launchable as a stdio MCP server.
- Output is deterministic and reproducible for a given vault state and query.
