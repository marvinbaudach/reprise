/**
 * The gate strip's state, as pure functions.
 *
 * The strip lets a visitor fail a check by clicking its mark, so the fail-closed
 * rule becomes something they trigger rather than something they are told. The
 * showroom suite has no DOM, so that behaviour is only checkable if the state
 * transition lives outside the component — the component stays a thin shell
 * around these two functions, the same shape `seekClock.ts` and `reveal.ts` use.
 */

export interface Readout {
  /** True while at least one check is failing. */
  readonly blocked: boolean;
  readonly failed: number;
  readonly total: number;
  /** The sentence the live region announces. */
  readonly message: string;
}

export function readout(failed: ReadonlySet<string>, total: number): Readout {
  if (!Number.isInteger(total) || total < 0) {
    throw new RangeError(`a wall of ${total} checks is not a wall`);
  }
  const count = failed.size;
  return {
    blocked: count > 0,
    failed: count,
    total,
    // The blocked half echoes the heading it sits under — "There is no partial
    // merge" — rather than reporting a count and leaving the consequence unsaid.
    message:
      count === 0
        ? `${total} checks green · ready to merge`
        : `${count} of ${total} red · the change does not land`,
  };
}

/** Show a reached check first; the merge verdict is the strip's resting copy. */
export function displayedReadout(status: Readout, index: number, name?: string): string {
  if (index < 0 || name === undefined) return status.message;
  return `${String(index + 1).padStart(2, '0')} · ${name}`;
}

/** Flip one check between passing and failing, without touching the set given. */
export function toggle(failed: ReadonlySet<string>, name: string): ReadonlySet<string> {
  const next = new Set(failed);
  if (!next.delete(name)) {
    next.add(name);
  }
  return next;
}
