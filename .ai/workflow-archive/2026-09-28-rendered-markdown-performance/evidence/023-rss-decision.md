# Task 023 RSS decision evidence

The unchanged realistic gate allows peak RSS up to 110% of the fixed
pre-incremental same-machine baseline. The source-injection parse tree is a
size-correlated retained owner: Task 020's controlled ablation removed about
30.7 MiB at 1 MiB, but also changed the first Rust delete from about 20.5 ms
to 708.1 ms. It cannot simply be dropped under the interaction requirement.

The narrow render change consumes highlighted code lines instead of cloning
them and computes source-line spans as those lines are consumed. A five-fresh-
process record for this candidate is in `023-rss-candidate.jsonl`. Its worst
Rust-fence RSS values were:

| Case | Baseline bytes | Candidate worst bytes | Baseline ratio | Current +10% ceiling bytes |
| --- | ---: | ---: | ---: | ---: |
| 128 KiB cold pane, 100×41 | 33,419,264 | 37,023,744 | 110.79% | 36,761,190 |
| 256 KiB layout | 51,515,392 | 57,077,760 | 110.80% | 56,666,931 |
| 512 KiB layout | 94,531,584 | 104,562,688 | 110.61% | 103,984,742 |
| 1 MiB layout | 180,727,808 | 200,142,848 | 110.74% | 198,800,589 |

The same 166-process record shows the non-Rust memory and retained-heap rows
under their existing limits; cold-time and layout-scaling measurements also
remain within limits. A proposed Rust-only +15% cap would leave the other
classes at +10% and provide 1.41, 2.16, 4.15, and 7.69 MB of headroom for
these four Rust rows respectively. This is a proposed limit, not an approved
or implemented gate change.

As a preliminary leak check on the exact 1 MiB mixed fixture, a fresh process
running 100 Rust delete/undo cycles peaked at 238.58 MiB and ended at 237.57
MiB. A separate fresh process running 400 cycles peaked at 237.82 MiB and
ended at 236.82 MiB; its four sample-quarter maxima were all 237.82 MiB.
This supports a bounded retained cache for that edit path, but it does not
establish leak-freedom across reload, mode transitions, or longer histories.
Those require an asserting stability gate before Task 023 can be complete.

Further exploratory attempts to stream syntax lines, reserve layout rows, and
reuse capture storage did not produce a durable margin under +10%; they were
removed. No temporary RSS tracing remains in product code. The last syntax
and layout changes are the narrow line-consumption path and reuse of the
already-maintained source line index.
