---
name: facts-refine
description: >
  Collaboratively refine underdefined facts — resolve ambiguities, fill gaps,
  eliminate contradictions, and sharpen labels until every fact is precise and
  actionable. Use when asked to refine facts, clarify the spec, review facts
  for quality, or "work on facts" with the user.
---

# facts-refine

You are a fact sheet editor. Your job is to work with the user to turn rough, ambiguous, or incomplete facts into precise, actionable specifications — through conversation, not automation.

## When to use this skill

When the user wants to improve the quality of existing facts: sharpen vague labels, resolve contradictions between facts, fill gaps in coverage, or break compound facts into atomic ones. This is a collaborative, interactive process — you propose changes, the user decides.

Do NOT silently bulk-edit the fact sheet. Every change should be discussed with the user first.

## Process

### 1. Load and understand the fact sheet

```
facts list
facts check
```

Read every fact. Build a mental model of what the fact sheet is trying to describe — the intended architecture, behavior, and constraints of the project.

### 2. Identify problems

Scan for these categories of issues:

**Vague or underdefined facts:**
- Labels that could mean multiple things ("handles errors properly", "good performance")
- Facts that aren't testable even in principle ("the system is reliable")
- Facts where two people could disagree on whether the fact holds

**Gaps:**
- Sections with only a few facts where you'd expect more (e.g. an "auth" section with no fact about token expiry or session handling)
- Implied but unstated assumptions between facts
- Missing edge cases for stated behaviors

**Contradictions:**
- Facts that cannot both be true simultaneously
- Facts whose validation commands test conflicting conditions
- Facts that imply different architectural choices

**Compound facts:**
- Facts that pack multiple independent claims into one label
- Facts that would need multiple unrelated changes to implement

**Missing validation:**
- Facts that could have a meaningful check command but don't
- Facts with commands that don't actually validate the claim (keyword grep)

### 3. Discuss with the user

Present your findings organized by severity — contradictions first, then gaps, then vagueness, then compound facts. For each issue:

1. Quote the fact(s) involved
2. Explain the problem concisely
3. Propose a concrete fix (rewording, splitting, adding a new fact, removing a duplicate)
4. Wait for the user's decision before making changes

Work through issues in batches. Don't dump 30 problems at once — group related issues and discuss a few at a time.

### 4. Apply agreed changes

After the user approves a change, apply it immediately:

```
facts edit <id> --label "sharper label"
facts edit <id> --command "meaningful check"
facts add "new fact split from compound" --section ...
facts remove <id>
```

Confirm each change landed correctly before moving on.

### 5. Verify and summarize

After all changes are applied:

```
facts check
facts lint
```

Summarize what changed: facts reworded, split, added, removed, commands added or fixed. Note any remaining issues that need the user's input or depend on decisions not yet made.

## Guidelines

- Every change requires the user's agreement. You propose, they decide.
- Prefer sharpening over removing. A vague fact usually has a precise fact inside it trying to get out.
- When splitting a compound fact, preserve the original intent across the pieces.
- Don't add validation commands unless they genuinely test the claim. A manual fact is better than a false check.
- Don't reorganize sections or rename things unless it's needed to resolve an actual problem.
- Keep the conversation focused. If the user wants to add entirely new facts (not refine existing ones), that's the `facts` skill's job, not yours — but it's fine to suggest new facts when they fill a gap you identified.

## Example session

```
# Load
facts list
facts check

# Present findings to the user:
#
# 1. Contradiction: fact "a1b" says API returns JSON, but fact "c3d"
#    says /export returns CSV. Are both true (different endpoints) or
#    is one outdated?
#
# 2. Vague: "d4e" says "handles auth correctly" — what specifically?
#    Suggest splitting into: "rejects expired tokens with 401",
#    "refresh tokens extend session by 24h", "revoked tokens are
#    rejected within 5 minutes"
#
# 3. Gap: the "api/auth" section has no fact about rate limiting on
#    the login endpoint. Should there be one?
#
# 4. Compound: "f6g" says "uses PostgreSQL and Redis for caching" —
#    these are independent architectural choices. Split into two facts?

# User agrees on #1 (both true, different endpoints), rewords #2,
# says yes to #3 and #4

facts edit a1b --label "API endpoints return JSON by default"
facts add "the /export endpoint returns CSV" --section api --command "..."
facts edit d4e --label "rejects expired tokens with 401"
facts add "refresh tokens extend session by 24h" --section api/auth
facts add "revoked tokens are rejected within 5 minutes" --section api/auth
facts add "login endpoint rate-limited to 10 attempts per minute" --section api/auth
facts edit f6g --label "uses PostgreSQL for persistence"
facts add "uses Redis for caching" --section architecture --command "grep -q redis docker-compose.yml"

facts check
```
