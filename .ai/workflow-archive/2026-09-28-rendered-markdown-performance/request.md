# Request: Rendered Markdown performance for standalone and embedded use

We recently did significant work to make it so that other applications could build on-top of `oom-edit` and use `oom-edit` as a markdown editor. One of those projects, during planning, observed several performance concerns:

> 1. Opening and editing large notes is slow, and it freezes the whole app (NFR-005)
>
> oom-edit's budget is 350 ms from opening a note to its first rendered frame. Measured end to end the way oom-edit defines it, realistic notes cross that at these sizes:
> - many small code blocks: from 16 KiB;
> - one large Rust code block: from 128 KiB;
> - mixed Markdown like the test vault's: from 144 KiB;
> - lists: from 256 KiB;
> - tables: from 448 KiB;
> - plain prose: never, even at 1 MiB.
>
> Among your five largest real notes, one (162 KB, code-heavy) takes 454 ms; the other four take 177–327 ms.
>
> The same cost comes back on every save, every edit that returns to rendered view, every window-width change, and every reload after sync. Switching tabs, searching and changing theme don't trigger it. Because the pane draws on the main thread, the whole app freezes for that time, with no way to show progress. A 1 MiB mixed note freezes it for 2.1–2.3 seconds each time.
>
> Two inefficiencies inside oom-edit cause it, and oom can't avoid them through oom-edit's public API.
> - Every code block recompiles its syntax-highlighting query on every layout, 18.7 ms per Rust block. That's 86–88% of the time on code-heavy notes. oom-edit's source view already caches these queries; the rendered view doesn't.
> - Mapping the cursor position does a scan whose cost grows with the square of the document size.
>
> As an experiment on a scratch copy only, fixing just those two brought your five largest notes to 120–155 ms. Every content type up to 512 KiB passed except one huge code block. A save on a 144 KiB mixed note dropped from about 311 ms to 35–52 ms.

Perform an in-depth investigation of the report. Determine whether it is accurate and identify an architecturally sound approach to fixing the issues. Preserve both core usage models:

1. `oom-edit` is a first-class standalone Markdown viewer and editor.
2. Other Rust applications can easily incorporate it as their Markdown viewer and editor, with the same fidelity and functionality as the standalone version.

Be test-forward in all bug fixes. The integrating `oom` project is on this machine, so its performance numbers were measured on this machine.

## Plan-change feedback — Revision 2

The user explicitly invoked `$feature-workflow revise` and requested the retained,
incremental `oom-edit-core` document-to-view architecture recommended after the
interaction investigation. The user rejected interstitial loading screens and
expressed concern that deleting a single line in a 1 MiB document currently
causes an observable approximately 600 ms pause. Responsiveness while editing
and scrolling takes priority over shaving cold first-render time to the old
350 ms budget; approximately 500 ms for a first render may be acceptable if
normal use stays responsive. Both standalone and embedded use must retain the
same exact functionality and fidelity.

The requested direction is:

> On an edit, update the authoritative text once and determine which Markdown
> blocks and document-wide relationships actually changed. Reparse and rebuild
> affected material; retain unchanged source analysis, rendered blocks, source
> mappings, and wrapped rows. Index row counts and source ranges so cursor
> mapping and viewport drawing do not require a full-document rebuild. Width
> changes would reflow affected visible material under the same model. Keep
> the standalone pane and embedded hosts on the same core path. No loading
> mode, no stale layout used for editing, and no second mutable document.

The user further requested a bounded, test-forward feasibility prototype
before the full refactor: instrument parsing propagation for representative
line deletions and delimiter edits; retain and rebuild a small set of blocks;
compare every cell, semantic style, source atom, cursor position, and operation
with the current full-build reference; and use an initial target of under
50 ms p99 from key to completed public-pane frame for ordinary 1 MiB edits.
If the prototype cannot approach that target while preserving fidelity, report
the result before expanding the implementation. The user wants critical
behavior instrumented and measured, no regressions, and no dependency or
parser replacement based only on hope. Broadly invalidating Markdown edits
such as removing a fence delimiter may require widespread work; do not claim a
universal latency ceiling without proof.

The workflow was in `PLAN_CHANGE_REQUIRED`, a phase omitted from the skill's
standard `revise` transition. The user explicitly authorized a safe transition
to planning that preserves completed tasks 001–005 and their evidence. This
authorization does not approve the revised implementation plan; a fresh
`$feature-workflow approve` is still required.

## Plan-change feedback — Revision 3

After task 011 measured the first-ever source-to-rendered transition separately
from repeated returns, the user accepted this recommendation:

> Keep <50 ms for every warmed return and give the first-ever
> source-to-rendered frame its own cold budget. This sacrifices no functionality
> and adds no loading state; a large note has one visible ~225 ms transition
> when opened directly in source view.

The user clarified that a large note with this one-time ~225 ms cost, followed
by the expected responsive operation, is acceptable. The user then explicitly
authorized moving the paused workflow back to planning to revise only the
unfinished acceptance work while preserving completed tasks 001–010 and their
evidence. This is authorization to prepare the revised plan, not approval to
resume product implementation; a fresh `$feature-workflow approve` is required.

## Acceptance follow-up — Round 2

### Original acceptance feedback (verbatim)

~~~text
I'm seeing a very visible hitch in scrolling up and down. But I can't seem to pin down why. It seems to happen if I am navigating more frequently up and down. Its easier to reproduce on the 1mb kitchen sink example. At arounds line 500 I start to get visible hitches - however if I stop and then resume scrolling it takes 400ish lines before visible hitches start showing up again.

Edit: Confirmed. If I use the version of `oom-edit` from `main` - it did not exhibit any of these visual hitches. Which further prooves that while the benchmarks may look good the experience isn't great. I also noticed (again using the new code changes) that if I open the 1mb file and try to scroll right after opening the visual hitches are even more pronounced.

Bugs I have found:

1. When deleting a block of text (like in a code fence) the text content is removed - however it leaves a block of empty lines in it's place.

2. When selecting text via visual mode the selection UI becomes unusably slow with 1mb file. This is true with the code changes in this branch or the code that exists in `main`.

Feedback:

Deleting code from the 1mb file is still way too slow. The good news is that our changes in this branch **are** better than what's in `main`. But that's a win without a big distinction. The user experience still sucks in either code version.

One other observation that I think is worth noting:

1. The code version from main behaves better when scrolling around - that's even true when I have to wait 500 to 600ms for the file to load.

2. The code version from `main` appears to use less memory when navigating a large file - but it's not by a lot. 160mb (main) 172mb (this code). That's likely not enough to chase a fix for.

----

My question to you is are we chasing the "sunk cost falacy" here? Is the solution we are chasing likely to actually get to performance that's acceptable, or do we need to step back and choose an alternate approach.
~~~

### Subsequent clarifications (verbatim)

~~~text
I tested in `normal mode` and `input mode`. Input didn't seem to have the same problems - though it's a bit harder to tell as the input mode treats scrolling around wrapped text differently than the normal mode (by design).

I was using keyboard navigation - not the mouse.

For the empty line deletion - I scrolled down 400 to 600 lines until I was into one of the large code fences. I use the visual/select mode to select a function. Then I hit the `d` key to delete. The text goes away - but the lines where the text previously existed still exist. Those lines should also be gone.

Lowercase `v`

I think the general rule to follow with `d` (delete) is:

If all of the visiable text on a line has been selected then also delete the line. Regardless of `v` or `V`. I will say - it's critical that we retain the current select --> yank/copy behavior.

I want to clarify something. I provided a text deletion example using a code-fence. The issue reproduces by deleting any text. I think the problem is more generalized than being related to code fences. That's just my guess though.

For these questions:

For wrapped Markdown, should `d` remove the physical source line only when all of its rendered rows are selected? I recommend yes; selecting one wrapped screen row should remove only its selected text.

Should the same whole-line rule apply to `c` (change), or only `d`? I recommend both for consistent delete/change behavior; yank/copy stays unchanged.

I agree with the first, I agree with the second.
~~~

### Planning interpretation

The user reports a very visible hitch when scrolling up and down repeatedly in
the roughly 1 MiB `examples/kitchen-sink-1mb.md` example. It becomes easier to
reproduce around line 500 after sustained navigation; stopping and resuming
appears to allow roughly another 400 lines before hitches return. Scrolling
immediately after opening makes the hitches more pronounced. The version from
`main` does not exhibit those hitches, despite its slower initial load. The user
considers this a regression not captured by the existing benchmarks.

The user also found:

1. Deleting a block of text, such as inside a code fence, removes its text but
   leaves a block of empty lines in its place.
2. Visual-mode text selection is unusably slow with a 1 MiB file, both on this
   branch and on `main`.
3. Deleting code in the 1 MiB file remains much too slow. This branch is better
   than `main`, but neither delivers acceptable interaction.

The user observed approximately 160 MB memory use on `main` and 172 MB on this
branch while navigating a large file, and does not consider that difference a
priority unless investigation shows it matters.

The user asks whether continuing this retained/incremental approach is a
sunk-cost error, whether it can plausibly reach acceptable interaction, or
whether the team should step back and choose an alternative. Responsiveness
and fidelity take precedence over the cold first-render budget. This revision
must investigate the evidence and make an explicit architectural go/no-go
recommendation before any new product-code changes are approved.

The user clarified the reproduction: the scrolling hitch was tested in
rendered Normal mode with keyboard navigation, not the mouse. Source Insert
mode did not seem to have the same problem, though its wrapped-text scrolling
differs by design. For deletion, the user navigated roughly 400–600 lines into
one of the large code fences, used rendered Select mode to select a function,
then pressed `d`. The function's text disappeared but its former physical
lines remained; the user expects those lines to be removed too.
The user specified that Select was started with lowercase `v` (characterwise
selection), not `V` or Ctrl-V.

The user then set the desired deletion rule: if all visible text on a line is
selected, `d` should also delete the line, regardless of whether Select began
with `v` or `V`. Existing Select → yank/copy behavior must remain unchanged.

The code-fence example is not an intended scope limit: the user reports that
the empty-line effect also occurs when deleting other text and suspects a
general selection/deletion problem. The user accepted these clarifications:
for a wrapped physical source line, remove the whole line only when all of
its rendered rows are selected; apply the same whole-line rule to `c` (change)
as to `d`; preserve current Select yank/copy behavior.

## Task-016 acceptance clarification — Revision 5

After exact-geometry pane and PTY measurements did not reproduce a branch-only
navigation stall, the user tested the explicit release binary built from this
branch in kitty and reported: "I'm not seeing any visual hitches or pauses when
using the binary that's built from our branch." The user's earlier `oom-edit`
command resolved to a separate clean-`main` release checkout, so that command
alone did not establish the branch's current behavior. No cause is assigned to
the earlier observation without evidence.

The user explicitly agreed and approved revising task 016 to retain sustained
1 MiB navigation regression checks, record the hitch as not currently
reproducible, and continue tasks 017–019. This approval does not relax the
Select responsiveness, large-fence edit latency, fidelity, or host-parity
requirements.

## Final-gate recovery feedback — Revision 6

The round-2 final gate exhausted its retry budget after the Rust-fence
peak-RSS comparisons exceeded the unchanged baseline-plus-10% ceilings.
The user explicitly authorized recovery of this active workflow and added:

> A 700ms pause is also not acceptable.

The latter refers to the documented roughly 690 ms mutation-handler cost of
deleting/changing a 15-line character selection across prose and Markdown
constructs in the exact 1 MiB kitchen-sink note. A rendered delete also
needed roughly 350 ms for the following complete frame. The authorized
recovery must address both the memory gate and this interactive pause without
weakening the fixed fidelity, host-parity, code-fence interaction, or realistic
memory gates. This message authorizes a safe transition out of
`RETRY_BUDGET_EXCEEDED` to investigate and write a revised plan; it does not
pre-approve new implementation details. A fresh explicit workflow approval is
required before changing product code.

## RSS-cap decision feedback — Revision 7

The user asked:

> What RSS gate are you chasing? If we have nailed the perf targets it might
> make sense to adjust the RSS targets.

After the measured Rust-only result and the trade-off with a slow first edit
were explained, the user clarified:

> I agree with your conclusion. I would much rather have the correct
> performance and adjust our RSS target up just a bit.
>
> The caveat to any of this is: Please make sure the extra RSS isn't due to a
> memory leak

The user then replied "Approved" to the request for workflow recovery and a
revision of the frozen plan. This authorizes revising the Rust-only RSS
criterion and adding an asserting memory-stability gate. It does not approve a
specific unreviewed plan hash or waive the leak check, fidelity, first-edit,
standalone/embedded, or other performance requirements.
