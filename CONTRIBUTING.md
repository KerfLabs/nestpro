# Contributing to nestpro

Thanks for helping. You don't need to write code to help: most of the useful
contributions come from people who run cutting machines every day.

## Ways to help

- **Report a problem.** Open an issue on GitHub. Say what you did, what you
  expected, and what happened instead: a wrong nest, a cut in the wrong place,
  a program your machine rejected.
- **Send test files.** DXF drawings, `.nxp` / `.mec` files or CNC programs that
  nestpro reads or nests wrongly are the most useful thing you can give us.
- **Ask for a machine.** Tell us which machine and controller you program, and
  what a working program for it looks like.
- **Fix or build something.** See *Code* below.

## Keep customer data out

The repository and its issues are **public**. Before you attach anything:

- Remove or rename customer names, part, drawing, job and PO numbers, people's
  names, server and share names, and e-mail addresses.
- If a file can't be cleaned, don't attach it. Write to **hello@kurflabs.com**
  instead and we'll arrange a private way to send it.

## Code

1. Read `AGENTS.md`. It describes how the project is laid out and the rules for
   keeping the public repository clean. They apply to people too.
2. Run `tools/install-hooks.sh` once after cloning. It checks every commit for
   shop data before it leaves your PC.
3. Run `tools/check.sh` before opening a pull request. It runs the same checks,
   Linux tests and Windows type-check as CI.
4. Open a pull request against `main`. Describe the change in shop terms
   (sheets, parts, cuts) first.

The app is used on Windows first. Don't break the Windows build to make
something work elsewhere.

## The contributor agreement (CLA)

Before your first pull request can be merged, you'll need to accept the
[Kurf Labs Contributor License Agreement](CLA.md). You do this once.

**Why we ask.** nestpro is free for small businesses and sold under a commercial
license to larger companies (see [LICENSING.md](LICENSING.md)). To include your
work in both, Kurf Labs needs your permission to license it under those terms,
and any future license. You keep the copyright to your work.

**How it works.** When you open your first pull request, a bot comments with a
link to the agreement. Read it and reply with the exact sentence the bot gives
you. The bot records your GitHub name and won't ask again.

**If you write code at work**, check with your employer first. Many employment
agreements give the employer the rights to code written on the job, and then it
is theirs to agree to, not yours.
