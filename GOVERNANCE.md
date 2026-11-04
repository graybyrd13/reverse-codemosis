# Governance

Reverse Codemosis is a commons: its value is the shared, reviewed set of
rules and facts that anyone can trust. Governance exists to protect that
trust, so it is open, written down and vendor-neutral.

## Roles

**Contributors** are anyone who opens an issue, reviews a change, or sends
a rule, a fact, a language, documentation or engine code.

**Reviewers** have shown sustained, careful work in an area (a pack, a
language, the engine, documentation). They can approve changes in that
area. Any maintainer can nominate a contributor; the maintainers decide by
lazy consensus.

**Maintainers** are responsible for the project as a whole: releases,
security, the acceptance contract and the fact vocabulary. They are
nominated by a maintainer after at least three months as a reviewer and
confirmed by a two-thirds vote of the maintainers. Maintainers who step back
become **emeritus** and keep their history and their voice in discussions.

No single company may hold more than half of the maintainer seats; when
that would happen, nominations from other organisations take precedence.

## Decisions

Most decisions are made in pull requests by lazy consensus: a change merges
when it has the required approvals and no unresolved objection.

| Change | Needs |
|---|---|
| A rule in a built-in pack | one reviewer of that pack; its proofs pass and lint is clean |
| A new fact, guard or language tier | two reviewers, one a maintainer; tests that show the fact is conservative |
| Anything in the acceptance contract, the pack format, a schema or the fingerprint | an RFC (below) |
| Releases and security advisories | a maintainer |

When consensus fails, the maintainers vote; a simple majority decides, and
the reasoning is written in the issue.

## RFCs

Changes to the pack format, the fact vocabulary's guarantees, the
acceptance contract, JSON schemas, fingerprints or this document go through
an RFC: copy [rfcs/0000-template.md](rfcs/0000-template.md), open a pull
request, and allow at least fourteen days for comment. The RFC is accepted
by maintainer consensus and merged with its number; implementation follows.

## Conduct

Be kind, be precise, assume good faith. Harassment or personal attacks are
not tolerated in any project space; maintainers may remove content and
contributors who repeat them. Report concerns privately to the maintainers
through a GitHub security advisory or the contact in SECURITY.md.

## Trademark

"Reverse Codemosis" names this project. Implementations and packs may say
they are "compatible with Reverse Codemosis" when they pass the conformance
and soundness suites of the version they name; they may not use the name in
a way that suggests they are the project or endorsed by it.
