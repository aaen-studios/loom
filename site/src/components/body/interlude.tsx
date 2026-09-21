/**
 * The interlude.
 *
 * ---------------------------------------------------------------------------
 * Why there is a full-bleed typographic turn in a manual
 * ---------------------------------------------------------------------------
 *
 * Eight sections of one-column prose is a wall, however well set each paragraph is.
 * The failure mode is real and specific: a reader gets four sections in and the
 * document stops having a shape, so the last four sections are skimmed or skipped.
 * Every well-made long document solves this, and they solve it the same way — a
 * change of register at the halfway point, which costs almost nothing and resets the
 * eye.
 *
 * A manual's version of this is a plate: a page that is mostly white, holding one
 * line of display type and nothing else. Which is exactly what this is. It is not a
 * section, it carries no information, and it is deliberately not in the contents —
 * it is a rest, and a rest is not an item.
 *
 * ---------------------------------------------------------------------------
 * The line, and why it is this line
 * ---------------------------------------------------------------------------
 *
 * It is the shortest true statement the document makes, and it is set at the only
 * display size on the page. Everything before it is the case for watching the model
 * work; everything after it is the detail of how. So the sentence that sits between
 * them is the sentence that summary is for, and it does not need to be supported by
 * anything — the eight sections on either side are the support.
 *
 * It is set in the darkest ink at the tightest tracking, on a rule that runs the full
 * width of the column and a little beyond it. The rule is the only ornament in the
 * entire document, which is precisely why it works: there is one, so it means
 * something.
 */
export function Interlude() {
  return (
    <aside className="wide pt-16 pb-2 sm:pt-24" aria-hidden="true">
      <div className="border-t-2 border-[var(--ink)] pt-7">
        <p className="t-title" style={{ fontSize: "clamp(1.75rem, 4.6vw, 2.75rem)" }}>
          An agent you can watch work
          <span className="text-[var(--accent)]">.</span>
        </p>
        <p className="mt-4 max-w-[36rem] text-[0.9375rem] leading-[1.6] text-soft">
          Three sections have described the application and one turn of it. What
          follows is the reference half: what it is made of, what it can be connected
          to, where your data goes, and how to install it.
        </p>
      </div>
    </aside>
  );
}
