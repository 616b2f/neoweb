//! Unified in-page DOM text search, `<mark>` highlighting, and match stepping for ServoEngine.

use super::engine::{ServoWebPage, xml_escape};
use crate::engine::{SearchOptions, SearchResult};

#[derive(Debug, Clone, serde::Deserialize)]
pub struct SearchMatchPayload {
    pub current_match: usize,
    pub total_matches: usize,
    #[serde(default)]
    pub scroll_y: f32,
}

/// Script that cleanly unwraps all `<mark class="__neoweb_search_highlight">` elements
/// back into plain text nodes and normalizes adjacent text nodes.
pub const SEARCH_CLEAR_SCRIPT: &str = r#"
(function() {
    var marks = document.querySelectorAll('mark.__neoweb_search_highlight');
    var parents = [];
    for (var i = 0; i < marks.length; i++) {
        var m = marks[i];
        var parent = m.parentNode;
        if (parent) {
            parent.replaceChild(document.createTextNode(m.textContent || ''), m);
            if (parents.indexOf(parent) === -1) {
                parents.push(parent);
            }
        }
    }
    for (var j = 0; j < parents.length; j++) {
        parents[j].normalize();
    }
    window.__neoweb_search_state = null;
    return "cleared";
})()
"#;

/// Unified SpiderMonkey script generator handling both initial DOM search/highlighting
/// (`opts.find_next == false`) and stepping through active matches (`opts.find_next == true`),
/// with automatic fallback to re-highlighting if an SPA re-rendered the DOM or `:noh` cleared marks.
pub fn build_search_script(query: &str, opts: SearchOptions) -> String {
    let query_json = serde_json::to_string(query).unwrap_or_else(|_| "\"\"".to_string());
    let backward_lit = if opts.backward { "true" } else { "false" };
    let find_next_lit = if opts.find_next { "true" } else { "false" };
    let wrap_lit = if opts.wrap { "true" } else { "false" };
    let case_lit = if opts.case_sensitive { "true" } else { "false" };

    format!(
        r#"
(function() {{
    var query = {query_json};
    var backward = {backward_lit};
    var findNext = {find_next_lit};
    var wrap = {wrap_lit};
    var caseSensitive = {case_lit};

    var INACTIVE_STYLE = 'background-color: #e0af68 !important; color: #1a1b26 !important; padding: 0 !important; margin: 0 !important; border-radius: 2px !important; outline: none !important; display: inline !important;';
    var ACTIVE_STYLE = 'background-color: #ff9e64 !important; color: #1a1b26 !important; padding: 0 !important; margin: 0 !important; border-radius: 2px !important; outline: 2px solid #f7768e !important; display: inline !important;';

    function scrollMarkIntoView(mark) {{
        var curY = window.scrollY || window.pageYOffset || 0;
        var targetY = curY;
        if (mark && mark.getBoundingClientRect) {{
            var rect = mark.getBoundingClientRect();
            var viewH = window.innerHeight || (document.documentElement && document.documentElement.clientHeight) || 600;
            if (rect.top < 40 || rect.bottom > viewH - 40) {{
                targetY = Math.max(0, curY + rect.top - (viewH / 2) + (rect.height / 2));
                window.scrollTo(window.scrollX || 0, targetY);
            }}
        }}
        return window.scrollY || targetY || 0;
    }}

    // Fast path: step existing highlights when findNext is requested for the active query
    var state = window.__neoweb_search_state;
    if (findNext && state && state.query === query && state.caseSensitive === caseSensitive) {{
        var existingMarks = document.querySelectorAll('mark.__neoweb_search_highlight');
        if (state.total === 0 && existingMarks.length === 0) {{
            return JSON.stringify({{ current_match: 0, total_matches: 0, scroll_y: window.scrollY || 0 }});
        }}
        if (existingMarks.length > 0) {{
            var total = existingMarks.length;
            var prevIdx = (typeof state.currentIdx === 'number' && state.currentIdx >= 0 && state.currentIdx < total)
                ? state.currentIdx
                : 0;

            existingMarks[prevIdx].setAttribute('style', INACTIVE_STYLE);
            existingMarks[prevIdx].removeAttribute('data-neoweb-active');

            var nextIdx;
            if (backward) {{
                nextIdx = wrap ? ((prevIdx - 1 + total) % total) : Math.max(0, prevIdx - 1);
            }} else {{
                nextIdx = wrap ? ((prevIdx + 1) % total) : Math.min(total - 1, prevIdx + 1);
            }}

            var nextMark = existingMarks[nextIdx];
            nextMark.setAttribute('style', ACTIVE_STYLE);
            nextMark.setAttribute('data-neoweb-active', 'true');
            var scrolledY = scrollMarkIntoView(nextMark);

            state.currentIdx = nextIdx;
            state.total = total;

            return JSON.stringify({{
                current_match: nextIdx + 1,
                total_matches: total,
                scroll_y: scrolledY
            }});
        }}
    }}

    // Full pass: clear old highlights and merge split text nodes
    var oldMarks = document.querySelectorAll('mark.__neoweb_search_highlight');
    var dirtyParents = [];
    for (var i = 0; i < oldMarks.length; i++) {{
        var om = oldMarks[i];
        var p = om.parentNode;
        if (p) {{
            p.replaceChild(document.createTextNode(om.textContent || ''), om);
            if (dirtyParents.indexOf(p) === -1) {{
                dirtyParents.push(p);
            }}
        }}
    }}
    for (var j = 0; j < dirtyParents.length; j++) {{
        dirtyParents[j].normalize();
    }}

    if (!query || query.length === 0) {{
        window.__neoweb_search_state = null;
        return JSON.stringify({{ current_match: 0, total_matches: 0, scroll_y: window.scrollY || 0 }});
    }}

    var needle = caseSensitive ? query : query.toLowerCase();
    var qLen = needle.length;
    if (qLen === 0) {{
        window.__neoweb_search_state = null;
        return JSON.stringify({{ current_match: 0, total_matches: 0, scroll_y: window.scrollY || 0 }});
    }}

    // Collect visible text nodes containing at least one match
    var SKIP_TAGS = {{
        'SCRIPT': true, 'STYLE': true, 'NOSCRIPT': true,
        'TEMPLATE': true, 'TEXTAREA': true, 'HEAD': true, 'TITLE': true
    }};
    var textNodes = [];
    function collectTextNodes(node) {{
        if (!node) return;
        if (node.nodeType === 3) {{
            var val = node.nodeValue;
            if (val) {{
                var hay = caseSensitive ? val : val.toLowerCase();
                if (hay.indexOf(needle) !== -1) {{
                    textNodes.push(node);
                }}
            }}
            return;
        }}
        if (node.nodeType !== 1) return;
        var tag = (node.tagName || '').toUpperCase();
        if (SKIP_TAGS[tag]) return;
        if (node.id === '__neoweb_hints_overlay') return;
        if (window.getComputedStyle) {{
            var st = window.getComputedStyle(node);
            if (st && (st.display === 'none' || st.visibility === 'hidden')) {{
                return;
            }}
        }}
        var children = node.childNodes;
        for (var c = 0; c < children.length; c++) {{
            collectTextNodes(children[c]);
        }}
    }}
    collectTextNodes(document.body || document.documentElement);

    // Wrap matches in <mark class="__neoweb_search_highlight">
    var matches = [];
    for (var t = 0; t < textNodes.length; t++) {{
        var tNode = textNodes[t];
        var parent = tNode.parentNode;
        if (!parent) continue;
        var text = tNode.nodeValue || '';
        var hayText = caseSensitive ? text : text.toLowerCase();
        var pos = 0;
        var idx = hayText.indexOf(needle, pos);
        if (idx === -1) continue;

        var frag = document.createDocumentFragment();
        while (idx !== -1) {{
            if (idx > pos) {{
                frag.appendChild(document.createTextNode(text.substring(pos, idx)));
            }}
            var mark = document.createElement('mark');
            mark.className = '__neoweb_search_highlight';
            mark.setAttribute('style', INACTIVE_STYLE);
            mark.textContent = text.substring(idx, idx + qLen);
            frag.appendChild(mark);
            matches.push(mark);
            pos = idx + qLen;
            idx = hayText.indexOf(needle, pos);
        }}
        if (pos < text.length) {{
            frag.appendChild(document.createTextNode(text.substring(pos)));
        }}
        parent.replaceChild(frag, tNode);
    }}

    if (matches.length === 0) {{
        window.__neoweb_search_state = {{ query: query, caseSensitive: caseSensitive, currentIdx: -1, total: 0 }};
        return JSON.stringify({{ current_match: 0, total_matches: 0, scroll_y: window.scrollY || 0 }});
    }}

    var activeIdx = backward ? (matches.length - 1) : 0;
    var activeMark = matches[activeIdx];
    activeMark.setAttribute('style', ACTIVE_STYLE);
    activeMark.setAttribute('data-neoweb-active', 'true');
    var finalScrollY = scrollMarkIntoView(activeMark);

    window.__neoweb_search_state = {{
        query: query,
        caseSensitive: caseSensitive,
        currentIdx: activeIdx,
        total: matches.length
    }};

    return JSON.stringify({{
        current_match: activeIdx + 1,
        total_matches: matches.length,
        scroll_y: finalScrollY
    }});
}})()
"#
    )
}

/// Finds non-overlapping byte ranges `(start, end)` of `needle` inside `haystack`.
/// Multi-byte UTF-8 safe across arbitrary Unicode scalar values.
pub fn find_substring_ranges(
    haystack: &str,
    needle: &str,
    case_sensitive: bool,
) -> Vec<(usize, usize)> {
    if needle.is_empty() || haystack.is_empty() {
        return Vec::new();
    }

    if case_sensitive {
        let mut ranges = Vec::new();
        let mut offset = 0;
        while let Some(rel) = haystack[offset..].find(needle) {
            let start = offset + rel;
            let end = start + needle.len();
            ranges.push((start, end));
            offset = end;
        }
        return ranges;
    }

    let needle_chars: Vec<char> = needle.chars().flat_map(|c| c.to_lowercase()).collect();
    if needle_chars.is_empty() {
        return Vec::new();
    }
    let hay_chars: Vec<(usize, char)> = haystack.char_indices().collect();
    let mut ranges = Vec::new();
    let mut i = 0;

    while i < hay_chars.len() {
        let mut n_idx = 0;
        let mut h_idx = i;
        let mut matched = true;

        while n_idx < needle_chars.len() {
            if h_idx >= hay_chars.len() {
                matched = false;
                break;
            }
            for lc in hay_chars[h_idx].1.to_lowercase() {
                if n_idx >= needle_chars.len() || lc != needle_chars[n_idx] {
                    matched = false;
                    break;
                }
                n_idx += 1;
            }
            if !matched {
                break;
            }
            h_idx += 1;
        }

        if matched && n_idx == needle_chars.len() {
            let start_byte = hay_chars[i].0;
            let end_byte = if h_idx < hay_chars.len() {
                hay_chars[h_idx].0
            } else {
                haystack.len()
            };
            if end_byte > start_byte {
                ranges.push((start_byte, end_byte));
                i = h_idx;
                continue;
            }
        }
        i += 1;
    }

    ranges
}

/// Escapes `input` for SVG `<text>` rendering and wraps occurrences of the active search query
/// in highlighted `<tspan>` spans (`#ff9e64` for active match, `#e0af68` for other matches).
pub fn xml_escape_highlighted(
    input: &str,
    active_search: Option<&SearchResult>,
    case_sensitive: bool,
    match_counter: &mut usize,
) -> String {
    let Some(sr) = active_search.filter(|s| !s.query.is_empty() && s.total_matches > 0) else {
        return xml_escape(input);
    };

    let ranges = find_substring_ranges(input, &sr.query, case_sensitive);
    if ranges.is_empty() {
        return xml_escape(input);
    }

    let mut out = String::with_capacity(input.len() + ranges.len() * 56);
    let mut last_end = 0;

    for (start, end) in ranges {
        if start > last_end {
            out.push_str(&xml_escape(&input[last_end..start]));
        }
        *match_counter += 1;
        let fill = if *match_counter == sr.current_match {
            "#ff9e64"
        } else {
            "#e0af68"
        };
        let matched_escaped = xml_escape(&input[start..end]);
        out.push_str(&format!(
            r##"<tspan fill="{fill}" font-weight="bold">{matched_escaped}</tspan>"##
        ));
        last_end = end;
    }

    if last_end < input.len() {
        out.push_str(&xml_escape(&input[last_end..]));
    }

    out
}

/// Canonical text lines `(content, y_offset)` rendered on built-in `about:` pages and fallback `ServoWebPage`s.
pub fn collect_builtin_page_matches(
    uri: &str,
    page: Option<&ServoWebPage>,
    query: &str,
    case_sensitive: bool,
) -> Vec<f32> {
    if query.is_empty() {
        return Vec::new();
    }

    let lines: Vec<(String, f32)> = match uri {
        "about:blank" => vec![("about:blank".to_string(), 300.0)],
        "about:help" => vec![
            ("Neoweb Help & Keybindings".to_string(), 90.0),
            (
                "Vi-Style Modal Web Browser powered by Servo & Iced".to_string(),
                130.0,
            ),
            ("Normal Mode Navigation".to_string(), 180.0),
            (
                "j / k : Scroll down / up (supports counts like 5j)".to_string(),
                210.0,
            ),
            ("h / l : Scroll left / right".to_string(), 235.0),
            (
                "<C-o> / <C-i> : Session history back / forward".to_string(),
                260.0,
            ),
            ("gg / G : Scroll to top / bottom".to_string(), 285.0),
            ("Tabs & Links".to_string(), 335.0),
            ("t / d : Open new tab / Close tab".to_string(), 365.0),
            ("gt / gT : Next tab / Previous tab".to_string(), 390.0),
            (
                "f / F : Follow hint (current tab / background tab)".to_string(),
                415.0,
            ),
            ("Commands & Search".to_string(), 465.0),
            (
                ": : Enter command prompt (:open, :tabopen, :session save)".to_string(),
                495.0,
            ),
            ("/ : Search forward, ? : Search backward".to_string(), 520.0),
        ],
        "about:neoweb" => vec![
            ("N".to_string(), 132.0),
            ("Neoweb".to_string(), 200.0),
            ("Servo + Iced + Lua Modal Web Browser".to_string(), 235.0),
            ("about:help".to_string(), 300.0),
            ("Keybinding reference".to_string(), 322.0),
            ("duckduckgo.com".to_string(), 300.0),
            ("Privacy search engine".to_string(), 322.0),
            ("github.com".to_string(), 300.0),
            ("GitHub platform".to_string(), 322.0),
        ],
        _ => {
            if let Some(p) = page {
                let mut v = Vec::new();
                let mut y = 50.0;
                v.push((p.title.clone(), y + 35.0));
                v.push((p.uri.clone(), y + 60.0));
                y += 95.0;
                for h in p.headings.iter().take(5) {
                    v.push((h.clone(), y));
                    y += 28.0;
                }
                if !p.headings.is_empty() {
                    y += 10.0;
                }
                for para in p.body_text.iter().take(8) {
                    v.push((para.clone(), y));
                    y += 22.0;
                }
                if !p.body_text.is_empty() {
                    y += 15.0;
                }
                if !p.links.is_empty() {
                    v.push(("Links & Navigation Targets:".to_string(), y));
                    y += 25.0;
                    for (label, href) in p.links.iter().take(25) {
                        let disp = if label.is_empty() {
                            href.clone()
                        } else {
                            format!("{label} → {href}")
                        };
                        v.push((disp, y + 19.0));
                        y += 34.0;
                    }
                }
                v
            } else {
                vec![(format!("Connecting to {uri}"), 75.0)]
            }
        }
    };

    let mut match_positions = Vec::new();
    for (line, y) in &lines {
        let count = find_substring_ranges(line, query, case_sensitive).len();
        for _ in 0..count {
            match_positions.push(*y);
        }
    }
    match_positions
}
