//! Link hinting scripts and JSON schemas for follow mode.

pub const HINT_KEYS: &[&str] = &[
    "a", "s", "d", "f", "g", "h", "j", "k", "l", "q", "w", "e", "r", "t", "y", "u", "i", "o", "p",
    "z", "x", "c", "v", "b", "n", "m", "1", "2", "3", "4", "5", "6", "7", "8", "9",
];

#[derive(Debug, serde::Deserialize)]
pub struct HintJson {
    pub key: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub uri: String,
    pub x: f32,
    pub y: f32,
    #[serde(default)]
    pub width: f32,
    #[serde(default)]
    pub height: f32,
}

pub const HINT_EXTRACTION_SCRIPT: &str = r#"
(function() {
    var old = document.getElementById('__neoweb_hints_overlay');
    if (old) old.remove();

    var candidates = document.querySelectorAll('a[href], button, input:not([type=hidden]), textarea, select, [role=button], [role=link], [onclick]');
    var alphabet = 'asdfghjklqwertyuiopzxcvbnm';
    var hints = [];
    var visibleIdx = 0;

    var overlay = document.createElement('div');
    overlay.id = '__neoweb_hints_overlay';
    overlay.style.position = 'fixed';
    overlay.style.top = '0';
    overlay.style.left = '0';
    overlay.style.width = '100vw';
    overlay.style.height = '100vh';
    overlay.style.pointerEvents = 'none';
    overlay.style.zIndex = '2147483647';

    for (var i = 0; i < candidates.length && visibleIdx < 120; i++) {
        var el = candidates[i];
        var rect = el.getBoundingClientRect();
        if (rect.width > 0 && rect.height > 0 && rect.bottom > 0 && rect.top < window.innerHeight && rect.right > 0 && rect.left < window.innerWidth) {
            var style = window.getComputedStyle(el);
            if (style.visibility !== 'hidden' && style.display !== 'none' && style.opacity !== '0') {
                var key;
                if (visibleIdx < alphabet.length) {
                    key = alphabet[visibleIdx];
                } else {
                    var first = alphabet[Math.floor(visibleIdx / alphabet.length) - 1];
                    var second = alphabet[visibleIdx % alphabet.length];
                    key = first + second;
                }

                var href = el.href || el.getAttribute('href') || '';
                var label = (el.innerText || el.value || el.title || el.getAttribute('aria-label') || el.tagName).trim();
                if (label.length > 50) label = label.substring(0, 47) + '...';

                el.setAttribute('data-neoweb-hint', key);

                var badge = document.createElement('span');
                badge.className = '__neoweb_hint_badge';
                badge.setAttribute('data-hint-key', key);
                badge.innerText = key;
                badge.style.position = 'absolute';
                badge.style.left = Math.max(0, rect.left) + 'px';
                badge.style.top = Math.max(0, rect.top) + 'px';
                badge.style.background = '#e0af68';
                badge.style.color = '#1a1b26';
                badge.style.fontFamily = 'monospace';
                badge.style.fontSize = '12px';
                badge.style.fontWeight = 'bold';
                badge.style.padding = '1px 4px';
                badge.style.borderRadius = '3px';
                badge.style.border = '1px solid #1a1b26';
                badge.style.boxShadow = '0 2px 4px rgba(0,0,0,0.5)';
                badge.style.lineHeight = '14px';
                overlay.appendChild(badge);

                hints.push({
                    key: key,
                    label: label,
                    uri: href,
                    x: rect.left,
                    y: rect.top,
                    width: rect.width,
                    height: rect.height
                });

                visibleIdx++;
            }
        }
    }
    if (document.body) {
        document.body.appendChild(overlay);
    }
    return JSON.stringify(hints);
})()
"#;

pub const HINT_CLEANUP_SCRIPT: &str = r#"
(function() {
    var old = document.getElementById('__neoweb_hints_overlay');
    if (old) old.remove();
    var tagged = document.querySelectorAll('[data-neoweb-hint]');
    for (var i = 0; i < tagged.length; i++) {
        tagged[i].removeAttribute('data-neoweb-hint');
    }
})()
"#;
