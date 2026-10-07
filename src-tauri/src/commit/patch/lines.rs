use super::{Edit, Kind};

fn align_replacements(edits: Vec<Edit<'_>>) -> Vec<Edit<'_>> {
    let mut aligned = Vec::with_capacity(edits.len());
    let mut offset = 0;
    while offset < edits.len() {
        if edits[offset].kind == Kind::Context {
            aligned.push(edits[offset].clone());
            offset += 1;
            continue;
        }
        let end = edits[offset..]
            .iter()
            .position(|edit| edit.kind == Kind::Context)
            .map_or(edits.len(), |end| offset + end);
        let removed: Vec<_> = edits[offset..end]
            .iter()
            .filter(|edit| edit.kind == Kind::Remove)
            .collect();
        let added: Vec<_> = edits[offset..end]
            .iter()
            .filter(|edit| edit.kind == Kind::Add)
            .collect();
        for index in 0..removed.len().max(added.len()) {
            if let Some(edit) = removed.get(index) {
                aligned.push((*edit).clone());
            }
            if let Some(edit) = added.get(index) {
                aligned.push((*edit).clone());
            }
        }
        offset = end;
    }
    aligned
}

pub(super) fn diff_lines<'a>(old: &[&'a [u8]], new: &[&'a [u8]]) -> Result<Vec<Edit<'a>>, String> {
    let max = old.len() + new.len();
    if max == 0 {
        return Ok(Vec::new());
    }
    if max > 200_000 {
        return Err("Too many lines for partial staging".into());
    }
    let offset = max + 1;
    let mut frontier = vec![0isize; 2 * max + 3];
    let mut trace = Vec::new();
    for distance in 0..=max {
        if (distance + 1) * frontier.len() > 8_000_000 {
            return Err("Diff is too complex for partial staging; stage the file instead".into());
        }
        trace.push(frontier.clone());
        for diagonal in (-(distance as isize)..=distance as isize).step_by(2) {
            let index = (offset as isize + diagonal) as usize;
            let mut x = if diagonal == -(distance as isize)
                || (diagonal != distance as isize && frontier[index - 1] < frontier[index + 1])
            {
                frontier[index + 1]
            } else {
                frontier[index - 1] + 1
            };
            let mut y = x - diagonal;
            while x < old.len() as isize
                && y < new.len() as isize
                && old[x as usize] == new[y as usize]
            {
                x += 1;
                y += 1;
            }
            frontier[index] = x;
            if x == old.len() as isize && y == new.len() as isize {
                return Ok(align_replacements(backtrack(old, new, &trace, offset)));
            }
        }
    }
    Err("Could not build the selected diff".into())
}

fn backtrack<'a>(
    old: &[&'a [u8]],
    new: &[&'a [u8]],
    trace: &[Vec<isize>],
    offset: usize,
) -> Vec<Edit<'a>> {
    let (mut x, mut y) = (old.len() as isize, new.len() as isize);
    let mut edits = Vec::new();
    for (distance, frontier) in trace.iter().enumerate().rev() {
        let diagonal = x - y;
        let index = (offset as isize + diagonal) as usize;
        let previous = if diagonal == -(distance as isize)
            || (diagonal != distance as isize && frontier[index - 1] < frontier[index + 1])
        {
            diagonal + 1
        } else {
            diagonal - 1
        };
        let previous_x = frontier[(offset as isize + previous) as usize];
        let previous_y = previous_x - previous;
        while x > previous_x && y > previous_y {
            x -= 1;
            y -= 1;
            edits.push(Edit {
                kind: Kind::Context,
                bytes: old[x as usize],
            });
        }
        if distance == 0 {
            break;
        }
        if x == previous_x {
            y -= 1;
            edits.push(Edit {
                kind: Kind::Add,
                bytes: new[y as usize],
            });
        } else {
            x -= 1;
            edits.push(Edit {
                kind: Kind::Remove,
                bytes: old[x as usize],
            });
        }
    }
    edits.reverse();
    edits
}
