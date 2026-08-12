//! Specialized always-hoist implementation for the default threshold of one.
//!
//! Unlike the configurable counted path, this path records parsed link spans
//! once, keys candidates only by label, and rewrites directly from those spans.
//! It avoids pair-count hashing, a second parse, pair-set lookups, per-line
//! output allocations, and separate line-ending scans.

use super::rewrite::{append_definition, replacement_pair};
use super::scan::{definition_text, line_segments, parse_inline_link, step_fence};
use crate::tables::{split_terminator, strip_doc_prefix};
use memchr::memmem;
use smallvec::SmallVec;
use std::borrow::Cow;

const NO_BLOCK: usize = usize::MAX;

struct Scan<'a> {
    candidates: Candidates<'a>,
    occurrences: Occurrences,
    blocks: DocBlocks<'a>,
    rust_context: bool,
    line_ending: &'static str,
}

type Candidates<'a> = SmallVec<[Candidate<'a>; 16]>;

type DocBlocks<'a> = SmallVec<[DocBlock<'a>; 2]>;

type Occurrences = SmallVec<[Occurrence; 24]>;

struct Candidate<'a> {
    text: &'a str,
    /// First inline URL for this label. `None` means a pre-existing definition
    /// was seen, making every occurrence ineligible.
    url: Option<&'a str>,
    occurrences: usize,
    last_block: usize,
}

struct DocBlock<'a> {
    prefix: &'a str,
    end: usize,
    definition_start: usize,
    definition_end: usize,
}

struct Occurrence {
    start: usize,
    candidate: usize,
    block: usize,
}

/// Hoist every eligible inline link while parsing each occurrence only once.
pub(super) fn fix_links_one(input: &str) -> (Cow<'_, str>, Vec<(String, String)>) {
    // Every accepted inline shape has its closing bracket immediately followed
    // by `(`. This rejects link-free and reference-only input in one scan.
    if memmem::find(input.as_bytes(), b"](").is_none() {
        return (Cow::Borrowed(input), Vec::new());
    }

    let scan = scan(input);
    if scan.rust_context {
        rewrite_rust(input, scan)
    } else {
        rewrite_markdown(input, scan)
    }
}

/// Rewrite document-scoped Markdown links by copying gaps between saved spans.
fn rewrite_markdown<'a>(input: &'a str, scan: Scan<'a>) -> (Cow<'a, str>, Vec<(String, String)>) {
    let selected = scan
        .candidates
        .iter()
        .filter(|candidate| candidate.url.is_some())
        .count();
    if selected == 0 {
        return (Cow::Borrowed(input), Vec::new());
    }

    // Size references and definitions before allocating.
    let mut capacity = input.len();
    if !input.ends_with('\n') {
        capacity += scan.line_ending.len();
    }
    for candidate in &scan.candidates {
        if let Some(url) = candidate.url {
            capacity -= candidate.occurrences * (url.len() + 2);
            capacity += candidate.text.len() + url.len() + 4 + scan.line_ending.len();
        }
    }

    let mut output = String::with_capacity(capacity);
    let mut last = 0usize;
    for occurrence in &scan.occurrences {
        if !is_eligible(&scan, occurrence) {
            continue;
        }
        output.push_str(&input[last..occurrence.start]);
        output.push('[');
        output.push_str(scan.candidates[occurrence.candidate].text);
        output.push(']');
        let candidate = &scan.candidates[occurrence.candidate];
        last = occurrence.start
            + candidate.text.len()
            + candidate
                .url
                .expect("eligible candidate has an inline URL")
                .len()
            + 4;
    }
    output.push_str(&input[last..]);
    if !output.ends_with('\n') {
        output.push_str(scan.line_ending);
    }
    for candidate in &scan.candidates {
        if let Some(url) = candidate.url {
            append_definition(&mut output, "", candidate.text, url, scan.line_ending);
        }
    }

    debug_assert_eq!(output.len(), capacity);
    let pairs = build_pairs(
        scan.candidates
            .iter()
            .filter(|candidate| candidate.url.is_some()),
        selected,
    );
    (Cow::Owned(output), pairs)
}

/// Rewrite only doc-comment occurrences and insert definitions at each using
/// block's end. Saved block and occurrence indices make this a linear merge.
fn rewrite_rust<'a>(input: &'a str, mut scan: Scan<'a>) -> (Cow<'a, str>, Vec<(String, String)>) {
    let mut rewrite_count = 0usize;
    let mut hoisted: SmallVec<[usize; 16]> = SmallVec::new();
    let mut definitions: SmallVec<[usize; 24]> = SmallVec::new();
    let mut capacity = input.len();

    // Plan per-block definitions and output size.
    for occurrence in &scan.occurrences {
        if occurrence.block == NO_BLOCK {
            continue;
        }
        let candidate = &mut scan.candidates[occurrence.candidate];
        let Some(url) = candidate.url else {
            continue;
        };
        rewrite_count += 1;
        capacity -= url.len() + 2;

        if candidate.last_block == NO_BLOCK {
            // Report each label once.
            hoisted.push(occurrence.candidate);
        }
        if candidate.last_block != occurrence.block {
            // Add one definition per label per block.
            candidate.last_block = occurrence.block;
            let block = &mut scan.blocks[occurrence.block];
            if block.definition_start == block.definition_end {
                block.definition_start = definitions.len();
            }
            definitions.push(occurrence.candidate);
            block.definition_end = definitions.len();
            capacity += scan.blocks[occurrence.block].prefix.len()
                + candidate.text.len()
                + url.len()
                + 4
                + scan.line_ending.len();
        }
    }

    if rewrite_count == 0 {
        return (Cow::Borrowed(input), Vec::new());
    }
    for block in &scan.blocks {
        if block.definition_start != block.definition_end && !input[..block.end].ends_with('\n') {
            capacity += scan.line_ending.len();
        }
    }

    let mut output = String::with_capacity(capacity);
    let mut last = 0usize;
    let mut occurrence_index = 0usize;
    // Merge ordered occurrences and blocks in one pass.
    for (block_index, block) in scan.blocks.iter().enumerate() {
        while occurrence_index < scan.occurrences.len() {
            let occurrence = &scan.occurrences[occurrence_index];
            if occurrence.block == NO_BLOCK || occurrence.block < block_index {
                occurrence_index += 1;
                continue;
            }
            if occurrence.block != block_index {
                break;
            }
            if is_eligible(&scan, occurrence) {
                output.push_str(&input[last..occurrence.start]);
                output.push('[');
                output.push_str(scan.candidates[occurrence.candidate].text);
                output.push(']');
                let candidate = &scan.candidates[occurrence.candidate];
                last = occurrence.start
                    + candidate.text.len()
                    + candidate
                        .url
                        .expect("eligible candidate has an inline URL")
                        .len()
                    + 4;
            }
            occurrence_index += 1;
        }

        if block.definition_start == block.definition_end {
            continue;
        }
        output.push_str(&input[last..block.end]);
        last = block.end;
        if !output.ends_with('\n') {
            output.push_str(scan.line_ending);
        }
        for &candidate_index in &definitions[block.definition_start..block.definition_end] {
            let candidate = &scan.candidates[candidate_index];
            append_definition(
                &mut output,
                block.prefix,
                candidate.text,
                candidate
                    .url
                    .expect("rewritten candidate has an inline URL"),
                scan.line_ending,
            );
        }
    }
    output.push_str(&input[last..]);

    debug_assert_eq!(output.len(), capacity);
    let pairs = build_pairs(
        hoisted.iter().map(|&index| &scan.candidates[index]),
        hoisted.len(),
    );
    (Cow::Owned(output), pairs)
}

fn build_pairs<'a>(
    candidates: impl Iterator<Item = &'a Candidate<'a>>,
    count: usize,
) -> Vec<(String, String)> {
    let mut pairs = Vec::with_capacity(count);
    for candidate in candidates {
        let url = candidate.url.expect("reported candidate has an inline URL");
        pairs.push(replacement_pair(candidate.text, url));
    }
    pairs
}

#[inline]
fn is_eligible(scan: &Scan<'_>, occurrence: &Occurrence) -> bool {
    scan.candidates[occurrence.candidate].url.is_some()
}

/// Parse non-fenced inline links, existing definitions, doc blocks, and line
/// endings in one line pass. Candidate indices replace pair hashes downstream.
fn scan(input: &str) -> Scan<'_> {
    let mut candidates = Candidates::new();
    let mut occurrences = Occurrences::new();
    let mut blocks = DocBlocks::new();
    let mut current_prefix: Option<&str> = None;
    let mut current_block = NO_BLOCK;
    let mut fence_stack = Vec::new();
    let mut rust_context = false;
    let mut crlf = 0usize;
    let mut lf = 0usize;
    for (segment_start, segment) in line_segments(input) {
        let segment_end = segment_start + segment.len();
        let (content, term) = split_terminator(segment);
        if term.len() == 2 {
            crlf += 1;
        } else if term.len() == 1 {
            lf += 1;
        }

        let (prefix, body) = strip_doc_prefix(content);
        if prefix.is_empty() {
            current_prefix = None;
            current_block = NO_BLOCK;
        } else if current_prefix == Some(prefix) {
            if current_block != NO_BLOCK {
                blocks[current_block].end = segment_end;
            }
        } else {
            // Wait for a link before creating a doc block.
            current_prefix = Some(prefix);
            current_block = NO_BLOCK;
        }

        let fence_delimiter = step_fence(&mut fence_stack, body);
        if !prefix.is_empty() && (fence_delimiter || fence_stack.is_empty()) {
            rust_context = true;
        }
        if fence_delimiter || !fence_stack.is_empty() {
            continue;
        }

        let body_start = segment_start + prefix.len();
        if !body.contains('[') {
            continue;
        }

        if let Some(text) = definition_text(body) {
            let candidate = candidate_for_definition(text, &mut candidates);
            // Disqualify labels with existing definitions.
            candidates[candidate].url = None;
        }
        let mut i = 0usize;
        while let Some(relative) = body[i..].find('[') {
            let open = i + relative;
            if let Some((text, url, end)) = parse_inline_link(body, open) {
                if !prefix.is_empty() && current_block == NO_BLOCK {
                    // Start a doc block at its first inline link.
                    current_block = blocks.len();
                    blocks.push(DocBlock {
                        prefix,
                        end: segment_end,
                        definition_start: 0,
                        definition_end: 0,
                    });
                }
                if let Some(candidate) = candidate_for_link(text, url, &mut candidates) {
                    candidates[candidate].occurrences += 1;
                    occurrences.push(Occurrence {
                        start: body_start + open,
                        candidate,
                        block: current_block,
                    });
                }
                i = end;
            } else {
                i = open + 1;
            }
        }
    }

    let line_ending = if crlf > 0 && crlf >= lf { "\r\n" } else { "\n" };
    Scan {
        candidates,
        occurrences,
        blocks,
        rust_context,
        line_ending,
    }
}

#[inline]
fn candidate_for_definition<'a>(text: &'a str, candidates: &mut Candidates<'a>) -> usize {
    if let Some(index) = candidates
        .iter()
        .position(|candidate| candidate.text == text)
    {
        return index;
    }
    let index = candidates.len();
    candidates.push(Candidate {
        text,
        url: None,
        occurrences: 0,
        last_block: NO_BLOCK,
    });
    index
}

#[inline]
fn candidate_for_link<'a>(
    text: &'a str,
    url: &'a str,
    candidates: &mut Candidates<'a>,
) -> Option<usize> {
    if let Some(index) = candidates
        .iter()
        .position(|candidate| candidate.text == text)
    {
        // Keep conflicting targets inline.
        return (candidates[index].url == Some(url)).then_some(index);
    }
    let index = candidates.len();
    candidates.push(Candidate {
        text,
        url: Some(url),
        occurrences: 0,
        last_block: NO_BLOCK,
    });
    Some(index)
}
