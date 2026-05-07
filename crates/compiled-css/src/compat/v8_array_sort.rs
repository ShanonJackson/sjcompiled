//! Faithful Rust port of V8's `Array.prototype.sort` algorithm.
//!
//! ## Why this exists
//!
//! `packages/css/src/plugins/sort-shorthand-declarations.ts` calls
//! `nodes.sort(sortNodes)`. The comparator is **non-transitive** —
//! it returns `0` whenever either side is a node without a first
//! `Decl` (a comment, or a rule whose first child is itself a rule).
//! Given non-transitive input, the observable output of any stable
//! sort is algorithm-defined: V8's TimSort/PowerSort, JSC's merge
//! sort, and Rust's `slice::sort_by` all preserve different
//! relative orderings.
//!
//! AFM production runs `transformCss` under node V8. The Rust port
//! must produce byte-identical sheets+classNames. The only way to
//! guarantee that on every input — including non-transitive ones —
//! is to replicate V8's exact sort algorithm.
//!
//! ## Source
//!
//! Direct port of `v8/third_party/v8/builtins/array-sort.tq` (Torque
//! source for `Array.prototype.sort`). V8 implements **PowerSort** —
//! a TimSort variant with a CPython-derived merge-tree power
//! heuristic. The Torque source itself states it's a port of
//! CPython's `Objects/listobject.c`.
//!
//! Key V8 functions ported here:
//! - `BinaryInsertionSort` (line 584)
//! - `CountAndMakeRun` (line 650)
//! - `MergeAt` (line 707)
//! - `GallopLeft` / `GallopRight` (lines 785, 885)
//! - `MergeLow` / `MergeHigh` (lines ~991, ~1120)
//! - `ArrayPowerSortImpl` main loop (line ~1259)
//! - `NodePower` / `NodePower32` for merge-tree depth (line ~447)
//!
//! ## Calling convention
//!
//! [`v8_sort`] takes a `&mut [T]` and a comparator
//! `Fn(&T, &T) -> Ordering`. The comparator may be non-transitive;
//! the algorithm is defined for any total comparator and produces
//! a deterministic permutation of the input slice.
//!
//! ## Stability and identity
//!
//! V8's sort is stable: when two elements compare `Equal`, the one
//! that appeared earlier in the input also appears earlier in the
//! output, **provided the merge phase preserves that invariant**.
//! For transitive comparators this is straightforward. For
//! non-transitive ones the "earlier in input" relation is preserved
//! through the run-detection + merge tree, which is *exactly* what
//! a 1:1 port of the algorithm gives us — this is precisely the
//! reason we need the algorithm itself, not a vanilla stable sort.
//!
//! ## What this is NOT
//!
//! - Not optimised for speed beyond what V8 does — we mirror the
//!   algorithm including its galloping mode and min_run boost
//!   because both affect observable output on non-transitive
//!   comparators.
//! - Not for use outside the parity surface. Plain Rust code should
//!   continue to use `slice::sort_by` / `sort_by_key`. The only
//!   call site is `sort_shorthand_declarations`.

use std::cmp::Ordering;

/// Galloping mode entry threshold — V8 `kMinGallopWins = 7`
/// (`array-sort.tq:209`). After 7 consecutive wins by one side of
/// a merge, the algorithm switches to galloping mode.
const K_MIN_GALLOP_WINS: i32 = 7;

/// Maximum number of pending runs on the merge stack. V8's
/// `kMaxMergePending` is 86 (sufficient for arrays up to
/// 2^65 elements per the CPython analysis).
const K_MAX_MERGE_PENDING: usize = 86;

/// Public entry point. Sorts `slice` in place using V8's PowerSort
/// algorithm with `cmp` as the comparator.
pub fn v8_sort<T, F>(slice: &mut [T], mut cmp: F)
where
    T: Clone,
    F: FnMut(&T, &T) -> Ordering,
{
    let len = slice.len();
    if len < 2 {
        return;
    }

    // BinaryInsertionSort fast path for very small arrays. V8 uses
    // this only when `length < kMaxBinaryInsertionSortLength` (line
    // 1531), but in our use the recursive `sort-shorthand-
    // declarations` produces many tiny slices, and the BIS fast
    // path matches the main algorithm's output identically (BIS is
    // a sub-step of PowerSort).
    if len < 2 {
        return;
    }

    let mut state = SortState {
        min_gallop: K_MIN_GALLOP_WINS,
        runs: Vec::with_capacity(K_MAX_MERGE_PENDING),
        powers: Vec::with_capacity(K_MAX_MERGE_PENDING),
    };

    power_sort_impl(slice, &mut state, &mut cmp);
}

/// State carried across helper calls. Mirrors V8's `SortState` Heap
/// object but only the fields PowerSort actually needs at run time.
struct SortState {
    /// `min_gallop` — adjusted up/down by `MergeLow`/`MergeHigh` to
    /// control when galloping mode kicks in. Initialised to
    /// `K_MIN_GALLOP_WINS`.
    min_gallop: i32,
    /// Pending-runs stack (each entry: base index, length).
    runs: Vec<(usize, usize)>,
    /// Per-boundary "power" depths used by PowerSort's merge policy.
    /// `powers[i]` is the power of the boundary between `runs[i]`
    /// and `runs[i+1]`. The last slot's power is computed lazily
    /// when a new run is pushed.
    powers: Vec<i32>,
}

/// `BinaryInsertionSort(low, start, high)` — `array-sort.tq:584`.
///
/// `[low, start)` is already sorted; insert each element from
/// `[start, high)` into its correct position via binary search.
/// Stable: equal elements keep their input order because the search
/// returns the position immediately to the **right** of any equal
/// run (`order < 0 → right = mid`, else `left = mid + 1`).
fn binary_insertion_sort<T, F>(slice: &mut [T], low: usize, start_arg: usize, high: usize, cmp: &mut F)
where
    F: FnMut(&T, &T) -> Ordering,
{
    debug_assert!(low <= start_arg && start_arg <= high);
    let mut start = if low == start_arg { start_arg + 1 } else { start_arg };
    while start < high {
        // Find the insertion point for slice[start] in [low, start).
        let mut left = low;
        let mut right = start;
        // SAFETY: split borrows so we can pass `pivot: &T` and
        // `slice[mid]: &T` to the comparator simultaneously.
        while left < right {
            let mid = left + ((right - left) >> 1);
            // Borrow pivot and mid simultaneously by index — both
            // immutable, so no aliasing issue.
            let order = cmp(&slice[start], &slice[mid]);
            if order == Ordering::Less {
                right = mid;
            } else {
                left = mid + 1;
            }
        }
        debug_assert!(left == right);
        // Slide [left, start) right by one and place pivot at left.
        // We use `swap` rotation instead of memmove because T isn't
        // necessarily Copy.
        let mut p = start;
        while p > left {
            slice.swap(p, p - 1);
            p -= 1;
        }
        start += 1;
    }
}

/// `CountAndMakeRun(low, high)` — `array-sort.tq:650`.
///
/// Detects the run starting at `low`. If the run is descending
/// (strict `Greater`), reverses it in place. Returns the run length.
///
/// "Strict descending" matters for stability: a non-strict
/// descending run reversal would flip the order of equal elements.
fn count_and_make_run<T, F>(slice: &mut [T], low_arg: usize, high: usize, cmp: &mut F) -> usize
where
    F: FnMut(&T, &T) -> Ordering,
{
    debug_assert!(low_arg < high);
    let low = low_arg + 1;
    if low == high {
        return 1;
    }
    let mut run_length: usize = 2;
    // Determine ascending vs descending by comparing slice[low] to slice[low - 1].
    let initial_order = cmp(&slice[low], &slice[low - 1]);
    let is_descending = initial_order == Ordering::Less;

    let mut idx = low + 1;
    while idx < high {
        let order = cmp(&slice[idx], &slice[idx - 1]);
        if is_descending {
            if order != Ordering::Less {
                break;
            }
        } else {
            if order == Ordering::Less {
                break;
            }
        }
        run_length += 1;
        idx += 1;
    }

    if is_descending {
        slice[low_arg..low_arg + run_length].reverse();
    }

    run_length
}

/// `GallopLeft(array, key, base, length, hint)` — `array-sort.tq:785`.
///
/// Returns the offset `k` in `0..=length` such that
/// `slice[base + k - 1] < key <= slice[base + k]`. If `key` is equal
/// to existing elements, returns the offset of the **leftmost**
/// equal element (`<` semantics). Used by `MergeLow` to skip the
/// prefix of A that's already in place.
fn gallop_left<T, F>(slice: &[T], key: &T, base: usize, length: usize, hint: usize, cmp: &mut F) -> usize
where
    F: FnMut(&T, &T) -> Ordering,
{
    debug_assert!(length > 0);
    debug_assert!(hint < length);

    let mut last_ofs: i64 = 0;
    let mut offset: i64 = 1;

    let initial_order = cmp(&slice[base + hint], key);
    if initial_order == Ordering::Less {
        // slice[base + hint] < key — gallop right.
        let max_ofs = (length - hint) as i64;
        while offset < max_ofs {
            let probe = &slice[base + hint + offset as usize];
            if cmp(probe, key) != Ordering::Less {
                break;
            }
            last_ofs = offset;
            offset = (offset << 1) + 1;
            if offset <= 0 {
                offset = max_ofs;
            }
        }
        if offset > max_ofs {
            offset = max_ofs;
        }
        last_ofs += hint as i64;
        offset += hint as i64;
    } else {
        // key <= slice[base + hint] — gallop left.
        let max_ofs = (hint + 1) as i64;
        while offset < max_ofs {
            let probe = &slice[base + hint - offset as usize];
            if cmp(probe, key) == Ordering::Less {
                break;
            }
            last_ofs = offset;
            offset = (offset << 1) + 1;
            if offset <= 0 {
                offset = max_ofs;
            }
        }
        if offset > max_ofs {
            offset = max_ofs;
        }
        let tmp = last_ofs;
        last_ofs = hint as i64 - offset;
        offset = hint as i64 - tmp;
    }

    debug_assert!(-1 <= last_ofs && last_ofs < offset && offset <= length as i64);

    // Binary search in (last_ofs, offset]: find smallest m such that
    // slice[base + m] >= key.
    last_ofs += 1;
    while last_ofs < offset {
        let m = last_ofs + ((offset - last_ofs) >> 1);
        let order = cmp(&slice[base + m as usize], key);
        if order == Ordering::Less {
            last_ofs = m + 1;
        } else {
            offset = m;
        }
    }
    debug_assert!(last_ofs == offset);
    offset as usize
}

/// `GallopRight(array, key, base, length, hint)` — `array-sort.tq:885`.
///
/// Returns the offset `k` such that
/// `slice[base + k - 1] <= key < slice[base + k]`. If `key` equals
/// existing elements, returns the offset of the position
/// immediately **right** of the rightmost equal element. Used by
/// `MergeLow`/`MergeHigh` for stability.
fn gallop_right<T, F>(slice: &[T], key: &T, base: usize, length: usize, hint: usize, cmp: &mut F) -> usize
where
    F: FnMut(&T, &T) -> Ordering,
{
    debug_assert!(length > 0);
    debug_assert!(hint < length);

    let mut last_ofs: i64 = 0;
    let mut offset: i64 = 1;

    let initial_order = cmp(key, &slice[base + hint]);
    if initial_order == Ordering::Less {
        // key < slice[base + hint] — gallop left.
        let max_ofs = (hint + 1) as i64;
        while offset < max_ofs {
            let probe = &slice[base + hint - offset as usize];
            if cmp(key, probe) != Ordering::Less {
                break;
            }
            last_ofs = offset;
            offset = (offset << 1) + 1;
            if offset <= 0 {
                offset = max_ofs;
            }
        }
        if offset > max_ofs {
            offset = max_ofs;
        }
        let tmp = last_ofs;
        last_ofs = hint as i64 - offset;
        offset = hint as i64 - tmp;
    } else {
        // slice[base + hint] <= key — gallop right.
        let max_ofs = (length - hint) as i64;
        while offset < max_ofs {
            let probe = &slice[base + hint + offset as usize];
            if cmp(key, probe) == Ordering::Less {
                break;
            }
            last_ofs = offset;
            offset = (offset << 1) + 1;
            if offset <= 0 {
                offset = max_ofs;
            }
        }
        if offset > max_ofs {
            offset = max_ofs;
        }
        last_ofs += hint as i64;
        offset += hint as i64;
    }

    debug_assert!(-1 <= last_ofs && last_ofs < offset && offset <= length as i64);

    // Binary search.
    last_ofs += 1;
    while last_ofs < offset {
        let m = last_ofs + ((offset - last_ofs) >> 1);
        let order = cmp(key, &slice[base + m as usize]);
        if order == Ordering::Less {
            offset = m;
        } else {
            last_ofs = m + 1;
        }
    }
    debug_assert!(last_ofs == offset);
    offset as usize
}

/// `MergeLow(baseA, lengthA, baseB, lengthB)` — `array-sort.tq:991`.
///
/// In-place merge of two adjacent sorted runs A=[baseA, baseA+lengthA)
/// and B=[baseB, baseB+lengthB). Precondition: lengthA <= lengthB.
/// Allocates a temp array of size `lengthA`, copies A there, then
/// merges from left to right.
fn merge_low<T, F>(
    slice: &mut [T],
    base_a: usize,
    length_a_arg: usize,
    base_b: usize,
    length_b_arg: usize,
    state: &mut SortState,
    cmp: &mut F,
) where
    T: Clone,
    F: FnMut(&T, &T) -> Ordering,
{
    debug_assert!(length_a_arg > 0 && length_b_arg > 0);
    debug_assert!(base_a + length_a_arg == base_b);

    let mut length_a = length_a_arg;
    let mut length_b = length_b_arg;

    // Copy A to a temp Vec; merge from temp + B into the work array.
    let temp: Vec<T> = slice[base_a..base_a + length_a].to_vec();

    let mut dest = base_a;
    let mut cursor_temp: usize = 0;
    let mut cursor_b = base_b;

    // Move B[0] into place — known minimum.
    slice[dest] = slice[cursor_b].clone();
    dest += 1;
    cursor_b += 1;
    length_b -= 1;
    if length_b == 0 {
        // Drain temp.
        for i in 0..length_a {
            slice[dest + i] = temp[cursor_temp + i].clone();
        }
        return;
    }
    if length_a == 1 {
        // Drain B then place final A.
        for _ in 0..length_b {
            slice[dest] = slice[cursor_b].clone();
            dest += 1;
            cursor_b += 1;
        }
        slice[dest] = temp[cursor_temp].clone();
        return;
    }

    let mut min_gallop = state.min_gallop;
    'outer: loop {
        let mut nof_wins_a: i32 = 0;
        let mut nof_wins_b: i32 = 0;

        // Pairwise mode.
        loop {
            debug_assert!(length_a > 1 && length_b > 0);
            let order = cmp(&slice[cursor_b], &temp[cursor_temp]);
            if order == Ordering::Less {
                slice[dest] = slice[cursor_b].clone();
                dest += 1;
                cursor_b += 1;
                nof_wins_b += 1;
                length_b -= 1;
                nof_wins_a = 0;
                if length_b == 0 {
                    // Drain temp.
                    for i in 0..length_a {
                        slice[dest + i] = temp[cursor_temp + i].clone();
                    }
                    state.min_gallop = min_gallop;
                    return;
                }
                if nof_wins_b >= min_gallop {
                    break;
                }
            } else {
                slice[dest] = temp[cursor_temp].clone();
                dest += 1;
                cursor_temp += 1;
                nof_wins_a += 1;
                length_a -= 1;
                nof_wins_b = 0;
                if length_a == 1 {
                    // Drain B then place final A.
                    for _ in 0..length_b {
                        slice[dest] = slice[cursor_b].clone();
                        dest += 1;
                        cursor_b += 1;
                    }
                    slice[dest] = temp[cursor_temp].clone();
                    state.min_gallop = min_gallop;
                    return;
                }
                if nof_wins_a >= min_gallop {
                    break;
                }
            }
        }

        // Galloping mode.
        min_gallop += 1;
        let mut first_iteration = true;
        while nof_wins_a >= K_MIN_GALLOP_WINS || nof_wins_b >= K_MIN_GALLOP_WINS || first_iteration {
            first_iteration = false;
            debug_assert!(length_a > 1 && length_b > 0);
            min_gallop = (min_gallop - 1).max(1);
            state.min_gallop = min_gallop;

            // GallopRight on the temp array (A) for slice[cursor_b].
            let key_b = slice[cursor_b].clone();
            nof_wins_a = gallop_right(&temp, &key_b, cursor_temp, length_a, 0, cmp) as i32;
            debug_assert!(nof_wins_a >= 0);
            if nof_wins_a > 0 {
                let n = nof_wins_a as usize;
                for i in 0..n {
                    slice[dest + i] = temp[cursor_temp + i].clone();
                }
                dest += n;
                cursor_temp += n;
                length_a -= n;
                if length_a == 1 {
                    for _ in 0..length_b {
                        slice[dest] = slice[cursor_b].clone();
                        dest += 1;
                        cursor_b += 1;
                    }
                    slice[dest] = temp[cursor_temp].clone();
                    state.min_gallop = min_gallop;
                    return;
                }
                if length_a == 0 {
                    state.min_gallop = min_gallop;
                    return;
                }
            }
            slice[dest] = slice[cursor_b].clone();
            dest += 1;
            cursor_b += 1;
            length_b -= 1;
            if length_b == 0 {
                for i in 0..length_a {
                    slice[dest + i] = temp[cursor_temp + i].clone();
                }
                state.min_gallop = min_gallop;
                return;
            }

            // GallopLeft on the work array (B) for temp[cursor_temp].
            let key_a = temp[cursor_temp].clone();
            nof_wins_b = gallop_left(slice, &key_a, cursor_b, length_b, 0, cmp) as i32;
            debug_assert!(nof_wins_b >= 0);
            if nof_wins_b > 0 {
                let n = nof_wins_b as usize;
                // Copy slice[cursor_b..cursor_b+n] -> slice[dest..dest+n].
                // dest < cursor_b always (dest fills the consumed prefix of A),
                // so a forward copy is safe.
                for i in 0..n {
                    slice[dest + i] = slice[cursor_b + i].clone();
                }
                dest += n;
                cursor_b += n;
                length_b -= n;
                if length_b == 0 {
                    for i in 0..length_a {
                        slice[dest + i] = temp[cursor_temp + i].clone();
                    }
                    state.min_gallop = min_gallop;
                    return;
                }
            }
            slice[dest] = temp[cursor_temp].clone();
            dest += 1;
            cursor_temp += 1;
            length_a -= 1;
            if length_a == 1 {
                for _ in 0..length_b {
                    slice[dest] = slice[cursor_b].clone();
                    dest += 1;
                    cursor_b += 1;
                }
                slice[dest] = temp[cursor_temp].clone();
                state.min_gallop = min_gallop;
                return;
            }
        }
        // Penalise leaving galloping mode.
        min_gallop += 1;
        state.min_gallop = min_gallop;

        // Re-enter pairwise mode at the top.
        if false {
            break 'outer;
        }
    }
}

/// `MergeHigh(baseA, lengthA, baseB, lengthB)` — `array-sort.tq:~1120`.
///
/// In-place merge of two adjacent sorted runs A=[baseA, baseA+lengthA)
/// and B=[baseB, baseB+lengthB). Precondition: lengthA > lengthB.
/// Allocates a temp array of size `lengthB`, copies B there, then
/// merges from right to left.
fn merge_high<T, F>(
    slice: &mut [T],
    base_a: usize,
    length_a_arg: usize,
    base_b: usize,
    length_b_arg: usize,
    state: &mut SortState,
    cmp: &mut F,
) where
    T: Clone,
    F: FnMut(&T, &T) -> Ordering,
{
    debug_assert!(length_a_arg > 0 && length_b_arg > 0);
    debug_assert!(base_a + length_a_arg == base_b);

    let mut length_a = length_a_arg;
    let mut length_b = length_b_arg;

    // Copy B to temp; merge from temp + A into the work array,
    // moving from the right end leftward.
    let temp: Vec<T> = slice[base_b..base_b + length_b].to_vec();

    let mut dest: i64 = (base_b + length_b) as i64 - 1;
    let mut cursor_temp: i64 = length_b as i64 - 1;
    let mut cursor_a: i64 = (base_a + length_a) as i64 - 1;

    // Move A[end] into place — known maximum.
    slice[dest as usize] = slice[cursor_a as usize].clone();
    dest -= 1;
    cursor_a -= 1;
    length_a -= 1;
    if length_a == 0 {
        for i in 0..length_b {
            slice[(dest as usize) - i] = temp[(cursor_temp as usize) - i].clone();
        }
        return;
    }
    if length_b == 1 {
        // Drain A from right to left, then place final B.
        for _ in 0..length_a {
            slice[dest as usize] = slice[cursor_a as usize].clone();
            dest -= 1;
            cursor_a -= 1;
        }
        slice[dest as usize] = temp[cursor_temp as usize].clone();
        return;
    }

    let mut min_gallop = state.min_gallop;
    loop {
        let mut nof_wins_a: i32 = 0;
        let mut nof_wins_b: i32 = 0;

        loop {
            debug_assert!(length_a > 0 && length_b > 1);
            let order = cmp(&temp[cursor_temp as usize], &slice[cursor_a as usize]);
            if order == Ordering::Less {
                slice[dest as usize] = slice[cursor_a as usize].clone();
                dest -= 1;
                cursor_a -= 1;
                nof_wins_a += 1;
                length_a -= 1;
                nof_wins_b = 0;
                if length_a == 0 {
                    for i in 0..length_b {
                        slice[(dest as usize) - i] = temp[(cursor_temp as usize) - i].clone();
                    }
                    state.min_gallop = min_gallop;
                    return;
                }
                if nof_wins_a >= min_gallop {
                    break;
                }
            } else {
                slice[dest as usize] = temp[cursor_temp as usize].clone();
                dest -= 1;
                cursor_temp -= 1;
                nof_wins_b += 1;
                length_b -= 1;
                nof_wins_a = 0;
                if length_b == 1 {
                    for _ in 0..length_a {
                        slice[dest as usize] = slice[cursor_a as usize].clone();
                        dest -= 1;
                        cursor_a -= 1;
                    }
                    slice[dest as usize] = temp[cursor_temp as usize].clone();
                    state.min_gallop = min_gallop;
                    return;
                }
                if nof_wins_b >= min_gallop {
                    break;
                }
            }
        }

        // Galloping mode.
        min_gallop += 1;
        let mut first_iteration = true;
        while nof_wins_a >= K_MIN_GALLOP_WINS || nof_wins_b >= K_MIN_GALLOP_WINS || first_iteration {
            first_iteration = false;
            debug_assert!(length_a > 0 && length_b > 1);
            min_gallop = (min_gallop - 1).max(1);
            state.min_gallop = min_gallop;

            // GallopRight on slice[base_a..base_a+length_a] for temp[cursor_temp].
            let key_b = temp[cursor_temp as usize].clone();
            let k = gallop_right(slice, &key_b, base_a, length_a, length_a - 1, cmp);
            nof_wins_a = (length_a - k) as i32;
            debug_assert!(nof_wins_a >= 0);
            if nof_wins_a > 0 {
                let n = nof_wins_a as usize;
                // Move slice[cursor_a-n+1..=cursor_a] -> slice[dest-n+1..=dest].
                // We must move RIGHT-to-LEFT to avoid overwriting source
                // before reading.
                for i in 0..n {
                    slice[dest as usize - i] = slice[cursor_a as usize - i].clone();
                }
                dest -= n as i64;
                cursor_a -= n as i64;
                length_a -= n;
                if length_a == 0 {
                    for i in 0..length_b {
                        slice[(dest as usize) - i] = temp[(cursor_temp as usize) - i].clone();
                    }
                    state.min_gallop = min_gallop;
                    return;
                }
            }
            slice[dest as usize] = temp[cursor_temp as usize].clone();
            dest -= 1;
            cursor_temp -= 1;
            length_b -= 1;
            if length_b == 1 {
                for _ in 0..length_a {
                    slice[dest as usize] = slice[cursor_a as usize].clone();
                    dest -= 1;
                    cursor_a -= 1;
                }
                slice[dest as usize] = temp[cursor_temp as usize].clone();
                state.min_gallop = min_gallop;
                return;
            }

            // GallopLeft on temp[..length_b] for slice[cursor_a].
            let key_a = slice[cursor_a as usize].clone();
            let k = gallop_left(&temp, &key_a, 0, length_b, length_b - 1, cmp);
            nof_wins_b = (length_b - k) as i32;
            debug_assert!(nof_wins_b >= 0);
            if nof_wins_b > 0 {
                let n = nof_wins_b as usize;
                for i in 0..n {
                    slice[dest as usize - i] = temp[cursor_temp as usize - i].clone();
                }
                dest -= n as i64;
                cursor_temp -= n as i64;
                length_b -= n;
                if length_b == 1 {
                    for _ in 0..length_a {
                        slice[dest as usize] = slice[cursor_a as usize].clone();
                        dest -= 1;
                        cursor_a -= 1;
                    }
                    slice[dest as usize] = temp[cursor_temp as usize].clone();
                    state.min_gallop = min_gallop;
                    return;
                }
                if length_b == 0 {
                    state.min_gallop = min_gallop;
                    return;
                }
            }
            slice[dest as usize] = slice[cursor_a as usize].clone();
            dest -= 1;
            cursor_a -= 1;
            length_a -= 1;
            if length_a == 0 {
                for i in 0..length_b {
                    slice[(dest as usize) - i] = temp[(cursor_temp as usize) - i].clone();
                }
                state.min_gallop = min_gallop;
                return;
            }
        }
        min_gallop += 1;
        state.min_gallop = min_gallop;
    }
}

/// `MergeAt(i)` — `array-sort.tq:707`.
///
/// Merges runs at stack indices `i` and `i + 1`. Updates the stack
/// to remove the merged-away entry.
fn merge_at<T, F>(slice: &mut [T], i: usize, state: &mut SortState, cmp: &mut F)
where
    T: Clone,
    F: FnMut(&T, &T) -> Ordering,
{
    let stack_size = state.runs.len();
    debug_assert!(stack_size >= 2);
    debug_assert!(i == stack_size - 2 || i == stack_size - 3);

    let (base_a, length_a_initial) = state.runs[i];
    let (base_b, length_b_initial) = state.runs[i + 1];
    debug_assert!(length_a_initial > 0 && length_b_initial > 0);
    debug_assert!(base_a + length_a_initial == base_b);

    // Update bookkeeping: combined run replaces run `i`, and if we
    // merged the middle pair, slide the top-of-stack run down.
    state.runs[i] = (base_a, length_a_initial + length_b_initial);
    if stack_size >= 3 && i == stack_size - 3 {
        let top = state.runs[i + 2];
        state.runs[i + 1] = top;
    }
    state.runs.pop();

    // Use galloping search to skip the prefix of A that's already
    // ≤ B[0] and the suffix of B that's already ≥ A[end].
    let key_right = slice[base_b].clone();
    let k = gallop_right(slice, &key_right, base_a, length_a_initial, 0, cmp);
    let base_a = base_a + k;
    let length_a = length_a_initial - k;
    if length_a == 0 {
        return;
    }

    let key_left = slice[base_a + length_a - 1].clone();
    let length_b = gallop_left(slice, &key_left, base_b, length_b_initial, length_b_initial - 1, cmp);
    if length_b == 0 {
        return;
    }

    if length_a <= length_b {
        merge_low(slice, base_a, length_a, base_b, length_b, state, cmp);
    } else {
        merge_high(slice, base_a, length_a, base_b, length_b, state, cmp);
    }
}

/// `NodePower32(s1, n1, n2, n)` — `array-sort.tq:447`.
///
/// Computes the depth of a boundary in PowerSort's optimal merge tree.
/// We use the 32-bit version unconditionally — it's ~10 instructions
/// and the difference vs. the 64-bit fast path is invisible at our
/// input sizes (always small).
fn node_power_32(s1: i64, n1: i64, n2: i64, n: i64) -> i32 {
    let mut result: i32 = 0;
    let mut a: i64 = 2 * s1 + n1;
    let mut b: i64 = a + n1 + n2;

    loop {
        result += 1;
        if a >= n {
            a -= n;
            b -= n;
        } else if b >= n {
            break;
        }
        a <<= 1;
        b <<= 1;
        // Safety bound — unreachable in practice for valid inputs.
        if result > 64 {
            break;
        }
    }
    result
}

/// `ArrayPowerSortImpl` — `array-sort.tq:~1259`.
///
/// Main loop. Walks the slice left-to-right, detecting natural
/// runs, extending short runs to `min_run` via BIS, pushing onto
/// the merge stack, and merging according to PowerSort's
/// power-based policy. Final pass merges any remaining runs.
fn power_sort_impl<T, F>(slice: &mut [T], state: &mut SortState, cmp: &mut F)
where
    T: Clone,
    F: FnMut(&T, &T) -> Ordering,
{
    let length = slice.len();
    if length < 2 {
        return;
    }
    let length_i = length as i64;

    // Compute mr_step / mr_mask per CPython minrun_next() — see
    // `array-sort.tq:~1280`. Produces minRunLength in [32, 64) for
    // length >= 64 and equals length for shorter inputs (so a
    // length-N input with N<64 boils down to a single BIS pass).
    let mut mr_step: i64 = length_i;
    let mut mr_mask: i64 = 0;
    while mr_step >= 64 {
        mr_step >>= 1;
        mr_mask = (mr_mask << 1) | 1;
    }
    let mr_remainder = length_i & mr_mask;
    let mr_threshold = mr_mask + 1;
    let mut mr_accum: i64 = 0;

    let mut low: usize = 0;
    let mut remaining = length;

    while remaining != 0 {
        mr_accum += mr_remainder;
        let mut min_run_length = mr_step;
        if mr_accum >= mr_threshold {
            mr_accum -= mr_threshold;
            min_run_length += 1;
        }
        debug_assert!(length < 64 || (32 <= min_run_length && min_run_length <= 64));

        let mut current_run_length = count_and_make_run(slice, low, low + remaining, cmp);
        if (current_run_length as i64) < min_run_length {
            let forced = (min_run_length as usize).min(remaining);
            binary_insertion_sort(slice, low, low + current_run_length, low + forced, cmp);
            current_run_length = forced;
        }

        let stack_size = state.runs.len();
        if stack_size == 0 {
            state.runs.push((low, current_run_length));
            state.powers.push(0); // placeholder; updated when next run pushes
        } else {
            let (prev_base, prev_len) = state.runs[stack_size - 1];
            let p = node_power_32(prev_base as i64, prev_len as i64, current_run_length as i64, length_i);

            // Collapse runs whose prior boundary has higher power
            // than the new boundary.
            while state.runs.len() >= 2 && state.powers[state.runs.len() - 2] > p {
                let i = state.runs.len() - 2;
                merge_at(slice, i, state, cmp);
            }
            // Stamp the previous boundary's power.
            let last = state.runs.len() - 1;
            // `powers[last]` was the placeholder for the boundary
            // **between previous run and new one**; we now know it is `p`.
            // But after possibly merging above, the meaning of `powers[last]`
            // shifts — `powers[i]` is the power between `runs[i]` and
            // `runs[i+1]`. So we set `powers[last] = p` for the new boundary.
            if last < state.powers.len() {
                state.powers[last] = p;
            } else {
                state.powers.push(p);
            }
            state.runs.push((low, current_run_length));
            // Placeholder for the boundary between the NEW run and the
            // not-yet-pushed next run; will be updated next iteration.
            state.powers.push(0);
        }

        low += current_run_length;
        remaining -= current_run_length;
    }

    // Final merges: collapse remaining runs. V8's policy: if
    // `runs[n-1] < runs[n+1]` then merge `runs[n-1]` and `runs[n]`,
    // else merge `runs[n]` and `runs[n+1]`. Where `n = stack_size - 2`.
    while state.runs.len() > 1 {
        let stack_size = state.runs.len();
        let mut n = stack_size - 2;
        if n > 0 && state.runs[n - 1].1 < state.runs[n + 1].1 {
            n -= 1;
        }
        merge_at(slice, n, state, cmp);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run<T: Clone + std::fmt::Debug + PartialEq, F: FnMut(&T, &T) -> Ordering>(
        items: &[T],
        cmp: F,
    ) -> Vec<T> {
        let mut v = items.to_vec();
        v8_sort(&mut v, cmp);
        v
    }

    /// Smoke test: sorts integers ascendingly. With a transitive
    /// comparator this should match every other stable sort.
    #[test]
    fn sorts_integers() {
        let v = run(&[5, 2, 8, 1, 9, 3], |a, b| a.cmp(b));
        assert_eq!(v, vec![1, 2, 3, 5, 8, 9]);
    }

    /// Stability under transitive comparator.
    #[test]
    fn stable_for_equal_pairs() {
        let v = run(&[(1, "a"), (2, "b"), (1, "c"), (2, "d")], |a, b| a.0.cmp(&b.0));
        assert_eq!(v, vec![(1, "a"), (1, "c"), (2, "b"), (2, "d")]);
    }

    /// AFM fixture 02508 reproduction — children of `.table` with
    /// post-recursion buckets. V8's expected output is empirically
    /// observed by running `Array.prototype.sort` with the same
    /// comparator in node and dumping the result.
    #[test]
    fn matches_v8_on_fixture_02508_table_children() {
        // (input_index, name, bucket-or-sentinel). Sentinel `-999` =
        // "no decl" → comparator returns Equal.
        let items: Vec<(usize, &str, i64)> = vec![
            (0, "background-color", i64::MAX),
            (1, "box-sizing",       i64::MAX),
            (2, "font",             1),
            (3, "&--disabled",      i64::MAX),
            (4, "&--dynamic",       -999),
            (5, "&:not()",          4),
            (6, "&__table",         1),
            (7, "&__header",        1),
            (8, "&__body",          1),
        ];
        let cmp = |a: &(usize, &str, i64), b: &(usize, &str, i64)| {
            if a.2 == -999 || b.2 == -999 {
                Ordering::Equal
            } else {
                a.2.cmp(&b.2)
            }
        };
        let sorted = run(&items, cmp);
        let names: Vec<&str> = sorted.iter().map(|t| t.1).collect();
        // V8 order observed empirically:
        assert_eq!(
            names,
            vec![
                "font", "&__table", "&__header", "&__body", "&:not()",
                "background-color", "box-sizing", "&--disabled", "&--dynamic",
            ],
            "v8_sort output differs from observed V8 Array.sort output"
        );
    }

    /// Fixture 18 reproduction — comments interleaved with decls at
    /// top level. V8 leaves the input unchanged because run detection
    /// sees the whole input as one ascending run (every adjacent
    /// pair compares Equal under the non-transitive comparator).
    #[test]
    fn matches_v8_on_fixture_18_comments_interleave_decls() {
        let items: Vec<(usize, &str, i64)> = vec![
            (0, "/* leading */",  -999),
            (1, "color",          i64::MAX),
            (2, "/* between */",  -999),
            (3, "background",     1),
            (4, "/* trailing */", -999),
        ];
        let cmp = |a: &(usize, &str, i64), b: &(usize, &str, i64)| {
            if a.2 == -999 || b.2 == -999 {
                Ordering::Equal
            } else {
                a.2.cmp(&b.2)
            }
        };
        let sorted = run(&items, cmp);
        let names: Vec<&str> = sorted.iter().map(|t| t.1).collect();
        assert_eq!(
            names,
            vec!["/* leading */", "color", "/* between */", "background", "/* trailing */"],
            "v8_sort should preserve input order on a single-run non-transitive input"
        );
    }

    /// Descending input — gets reversed in run detection.
    #[test]
    fn descending_input_reverses() {
        let v = run(&[5, 4, 3, 2, 1], |a, b| a.cmp(b));
        assert_eq!(v, vec![1, 2, 3, 4, 5]);
    }

    /// Single-element / empty inputs are no-ops.
    #[test]
    fn handles_trivial_inputs() {
        assert_eq!(run::<i32, _>(&[], |a, b| a.cmp(b)), Vec::<i32>::new());
        assert_eq!(run(&[42], |a, b| a.cmp(b)), vec![42]);
    }

    /// Larger random input with transitive comparator — must produce
    /// the same result as a vanilla stable sort.
    #[test]
    fn matches_stable_sort_for_transitive_comparator() {
        let input: Vec<i32> = (0..200).rev().chain(0..50).chain(50..200).collect();
        let mut expected = input.clone();
        expected.sort();
        let actual = run(&input, |a, b| a.cmp(b));
        assert_eq!(actual, expected);
    }
}
