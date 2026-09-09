//! Count independent sites and suppress only fully covered contained sequences.

/// Equal normalized spans, in increasing source order.
pub(super) struct Group {
    pub(super) starts: Vec<usize>,
    pub(super) length: usize,
    pub(super) meaningful: usize,
}

impl Group {
    /// Require one common offset and coverage of every occurrence, not just overlap.
    pub(super) fn covers(&self, other: &Self) -> bool {
        if self.length <= other.length || self.starts.len() < other.starts.len() {
            return false;
        }
        let first = other.starts[0];
        self.starts
            .iter()
            .take_while(|&&start| start <= first)
            .any(|&start| {
                let offset = first - start;
                offset <= self.length - other.length
                    && other.starts.iter().all(|&site| {
                        site.checked_sub(offset)
                            .is_some_and(|parent| self.starts.binary_search(&parent).is_ok())
                    })
            })
    }
}

/// Keep shorter groups only when they have independent extra sites.
pub(super) fn consolidate(groups: &mut Vec<Group>) {
    groups.sort_unstable_by(|a, b| b.length.cmp(&a.length).then(a.starts.cmp(&b.starts)));
    let mut index = 0;
    while index < groups.len() {
        if groups[..index]
            .iter()
            .any(|longer| longer.covers(&groups[index]))
        {
            groups.remove(index);
        } else {
            index += 1;
        }
    }
}

/// Choose the earliest eligible site that belongs to a threshold-sized packing.
///
/// Equal-width intervals admit greedy prefix/suffix counts. Forcing the query
/// avoids losing it merely because an earlier overlapping occurrence was selected.
pub(super) fn select_sites(
    group: &Group,
    query_end: &[usize],
    minimum: usize,
) -> Option<(usize, Vec<usize>)> {
    if group.starts.len() < minimum {
        return None;
    }
    let starts = &group.starts;
    let length = group.length;
    let mut suffix = vec![0; starts.len() + 1];
    for index in (0..starts.len()).rev() {
        let next = starts.partition_point(|&start| start < starts[index] + length);
        suffix[index] = 1 + suffix[next];
    }

    let mut prefix_count = 0;
    let mut prefix_end = 0;
    let mut prefix_index = 0;
    let anchor = starts.iter().copied().find(|&query| {
        while prefix_index < starts.len() && starts[prefix_index] + length <= query {
            let start = starts[prefix_index];
            if start >= prefix_end {
                prefix_count += 1;
                prefix_end = start + length;
            }
            prefix_index += 1;
        }
        let after = starts.partition_point(|&start| start < query + length);
        query_end[query] >= query + length && prefix_count + 1 + suffix[after] >= minimum
    })?;

    let mut sites = Vec::new();
    let mut end = 0;
    for &start in starts {
        if start >= end && start + length <= anchor {
            sites.push(start);
            end = start + length;
        }
    }
    sites.push(anchor);
    end = anchor + length;
    for &start in starts {
        if start >= end {
            sites.push(start);
            end = start + length;
        }
    }
    Some((anchor, sites))
}
