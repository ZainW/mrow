//! Mrow's icons: the SF Symbols the Swift app used, redrawn as small SVG bodies on a 16 pt grid
//! with round 1.4 pt strokes, so they rasterise into Rune's glyph atlas on every platform.

use rune::Icon;

macro_rules! icon {
    ($name:ident, $id:literal, $body:literal) => {
        pub static $name: Icon = Icon { name: concat!("mrow-", $id), view: 16.0, body: $body };
    };
}

icon!(
    SIDEBAR,
    "sidebar",
    r#"<rect x="2" y="3" width="12" height="10" rx="2" stroke="currentColor" stroke-width="1.3" fill="none"/><path d="M6 3v10" stroke="currentColor" stroke-width="1.3"/><path d="M3.6 5.4h1M3.6 7.2h1" stroke="currentColor" stroke-width="1.1" stroke-linecap="round"/>"#
);
icon!(
    REFRESH,
    "refresh",
    r#"<path d="M13 8a5 5 0 1 1-1.6-3.7" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" fill="none"/><path d="M11.8 2v2.6H9.2" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#
);
icon!(
    SEARCH,
    "search",
    r#"<circle cx="7" cy="7" r="4.3" stroke="currentColor" stroke-width="1.4" fill="none"/><path d="M10.2 10.2L13.5 13.5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#
);
icon!(
    OPEN_EXTERNAL,
    "open-external",
    r#"<rect x="2.5" y="2.5" width="11" height="11" rx="2.4" stroke="currentColor" stroke-width="1.3" fill="none"/><path d="M6.4 9.6L10 6M7.1 6H10v2.9" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#
);
icon!(
    ARROW_UP_RIGHT,
    "arrow-up-right",
    r#"<path d="M4.5 11.5L11.5 4.5M5.5 4.5h6v6" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#
);
icon!(
    ELLIPSIS,
    "ellipsis",
    r#"<circle cx="3.5" cy="8" r="1.2" fill="currentColor"/><circle cx="8" cy="8" r="1.2" fill="currentColor"/><circle cx="12.5" cy="8" r="1.2" fill="currentColor"/>"#
);
icon!(
    PULL,
    "pull",
    r#"<circle cx="4.5" cy="3.6" r="1.5" stroke="currentColor" stroke-width="1.3" fill="none"/><circle cx="4.5" cy="12.4" r="1.5" stroke="currentColor" stroke-width="1.3" fill="none"/><circle cx="11.5" cy="12.4" r="1.5" stroke="currentColor" stroke-width="1.3" fill="none"/><path d="M4.5 5.1v5.8M11.5 10.9V6.6c0-1.1-.9-2-2-2H7.4M8.8 3.2L7.4 4.6l1.4 1.4" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#
);
icon!(
    BRANCH,
    "branch",
    r#"<circle cx="4.8" cy="3.4" r="1.4" stroke="currentColor" stroke-width="1.3" fill="none"/><circle cx="11.2" cy="3.4" r="1.4" stroke="currentColor" stroke-width="1.3" fill="none"/><path d="M4.8 4.8v8.4M11.2 4.8c0 3-6.4 2.6-6.4 6" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" fill="none"/>"#
);
icon!(CLOSE, "close", r#"<path d="M4.5 4.5l7 7M11.5 4.5l-7 7" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/>"#);
icon!(DISC, "disc", r#"<circle cx="8" cy="8" r="6.2" fill="currentColor"/>"#);
icon!(SMALL_X, "small-x", r#"<path d="M6 6l4 4M10 6l-4 4" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"/>"#);
icon!(
    CHEVRON_UP_DOWN,
    "chevron-up-down",
    r#"<path d="M5.5 6.2L8 3.7l2.5 2.5M5.5 9.8L8 12.3l2.5-2.5" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#
);
icon!(
    CHEVRON_DOWN,
    "chevron-down",
    r#"<path d="M4.5 6.3L8 9.8l3.5-3.5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#
);
icon!(
    CHEVRON_RIGHT,
    "chevron-right",
    r#"<path d="M6.3 4.5L9.8 8l-3.5 3.5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#
);
icon!(
    PERSON,
    "person",
    r#"<circle cx="8" cy="5.4" r="2.6" stroke="currentColor" stroke-width="1.3" fill="none"/><path d="M3 13.4c.4-2.4 2.5-3.9 5-3.9s4.6 1.5 5 3.9" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" fill="none"/>"#
);
icon!(
    PERSON_FILL,
    "person-fill",
    r#"<circle cx="8" cy="5.4" r="2.8" fill="currentColor"/><path d="M2.8 13.8c.3-2.7 2.5-4.4 5.2-4.4s4.9 1.7 5.2 4.4z" fill="currentColor"/>"#
);
icon!(
    SORT_DOWN,
    "sort-down",
    r#"<path d="M8 2.8v8M4.8 7.8L8 11l3.2-3.2M3.5 13.4h9" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#
);
icon!(
    SORT_UP,
    "sort-up",
    r#"<path d="M8 13.2v-8M4.8 8.2L8 5l3.2 3.2M3.5 2.6h9" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#
);
icon!(FILTER, "filter", r#"<path d="M2.5 4.5h11M4.5 8h7M6.5 11.5h3" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"/>"#);
icon!(
    GEAR,
    "gear",
    r#"<circle cx="8" cy="8" r="2.1" stroke="currentColor" stroke-width="1.3" fill="none"/><path d="M8 1.8l1 1.6 1.8-.4.5 1.8 1.8.5-.4 1.8 1.6 1-1.6 1 .4 1.8-1.8.5-.5 1.8-1.8-.4-1 1.6-1-1.6-1.8.4-.5-1.8-1.8-.5.4-1.8-1.6-1 1.6-1-.4-1.8 1.8-.5.5-1.8 1.8.4z" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round" fill="none"/>"#
);
icon!(
    APPEARANCE,
    "appearance",
    r#"<circle cx="8" cy="8" r="5.6" stroke="currentColor" stroke-width="1.3" fill="none"/><path d="M8 2.4a5.6 5.6 0 0 0 0 11.2z" fill="currentColor"/>"#
);
icon!(
    CHECK,
    "check",
    r#"<path d="M3.4 8.4l3 3 6.2-6.8" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#
);
icon!(
    UNDO,
    "undo",
    r#"<path d="M5.2 4.2L2.8 6.6 5.2 9M3 6.6h6.4a3.6 3.6 0 0 1 0 7.2H7" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#
);
icon!(
    COPY,
    "copy",
    r#"<rect x="5.2" y="5.2" width="8.3" height="8.3" rx="1.8" stroke="currentColor" stroke-width="1.3" fill="none"/><path d="M10.6 3.4c-.2-.6-.8-.9-1.4-.9H4c-.8 0-1.5.7-1.5 1.5v5.2c0 .7.4 1.2.9 1.4" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" fill="none"/>"#
);
icon!(
    MERGE,
    "merge",
    r#"<circle cx="4.5" cy="3.4" r="1.4" stroke="currentColor" stroke-width="1.3" fill="none"/><circle cx="4.5" cy="12.6" r="1.4" stroke="currentColor" stroke-width="1.3" fill="none"/><circle cx="11.5" cy="9" r="1.4" stroke="currentColor" stroke-width="1.3" fill="none"/><path d="M4.5 4.8v6.4M4.5 4.8c0 2.4 2 4.2 5.6 4.2" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" fill="none"/>"#
);
icon!(
    CLOSED_CIRCLE,
    "closed-circle",
    r#"<circle cx="8" cy="8" r="5.6" stroke="currentColor" stroke-width="1.3" fill="none"/><path d="M6 6l4 4M10 6l-4 4" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/>"#
);
icon!(DASHED_CIRCLE, "dashed-circle", r#"<circle cx="8" cy="8" r="5.6" stroke="currentColor" stroke-width="1.3" stroke-dasharray="2.2 2" fill="none"/>"#);
icon!(
    BUBBLE_TEXT,
    "bubble-text",
    r#"<path d="M2.6 4c0-.9.7-1.6 1.6-1.6h7.6c.9 0 1.6.7 1.6 1.6v5.2c0 .9-.7 1.6-1.6 1.6H7l-3 2.6v-2.6h0c-.8 0-1.4-.7-1.4-1.6z" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" fill="none"/><path d="M5.2 5.4h5.6M5.2 7.6h3.6" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>"#
);
icon!(
    EXCLAIM_CIRCLE,
    "exclaim-circle",
    r#"<circle cx="8" cy="8" r="5.6" stroke="currentColor" stroke-width="1.3" fill="none"/><path d="M8 4.9v3.6" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/><circle cx="8" cy="10.9" r=".85" fill="currentColor"/>"#
);
icon!(
    SMALL_CHECK,
    "small-check",
    r#"<path d="M5.3 8.2l1.8 1.8 3.6-3.9" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#
);
icon!(
    UNDO_CIRCLE,
    "undo-circle",
    r#"<circle cx="8" cy="8" r="5.6" stroke="currentColor" stroke-width="1.3" fill="none"/><path d="M6.4 5.6L5 7l1.4 1.4M5.2 7h3.4a1.8 1.8 0 0 1 0 3.6H7.4" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#
);
icon!(
    LEFT_HALF_CIRCLE,
    "left-half-circle",
    r#"<circle cx="8" cy="8" r="5.6" stroke="currentColor" stroke-width="1.3" fill="none"/><path d="M8 2.4a5.6 5.6 0 0 0 0 11.2z" fill="currentColor"/>"#
);
icon!(PLAIN_CIRCLE, "plain-circle", r#"<circle cx="8" cy="8" r="5.6" stroke="currentColor" stroke-width="1.3" fill="none"/>"#);
icon!(
    WARNING,
    "warning",
    r#"<path d="M8 2.6l5.8 10.2H2.2z" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" fill="none"/><path d="M8 6.6v3" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"/><circle cx="8" cy="11.3" r=".8" fill="currentColor"/>"#
);
icon!(
    SEAL_CHECK,
    "seal-check",
    r#"<path d="M8 1.8l1.5 1.2 1.9-.2.6 1.8 1.6 1-.6 1.8.6 1.8-1.6 1-.6 1.8-1.9-.2L8 14.2l-1.5-1.2-1.9.2-.6-1.8-1.6-1 .6-1.8-.6-1.8 1.6-1 .6-1.8 1.9.2z" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round" fill="none"/><path d="M5.8 8.1l1.5 1.5 2.9-3.1" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#
);
icon!(
    TRAY,
    "tray",
    r#"<path d="M2.5 9.2l1.8-5.4c.1-.4.5-.6.9-.6h5.6c.4 0 .8.2.9.6l1.8 5.4v3c0 .6-.4 1-1 1h-9c-.6 0-1-.4-1-1z" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" fill="none"/><path d="M2.5 9.2h3l.8 1.4h3.4l.8-1.4h3" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" fill="none"/>"#
);
icon!(
    PENCIL_SQUARE,
    "pencil-square",
    r#"<path d="M7.6 2.8H4.2c-.8 0-1.4.6-1.4 1.4v7.6c0 .8.6 1.4 1.4 1.4h7.6c.8 0 1.4-.6 1.4-1.4V8.4" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" fill="none"/><path d="M11.8 2.6l1.6 1.6-5.6 5.6-2.1.5.5-2.1z" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" fill="none"/>"#
);
icon!(
    DOC_PLUS,
    "doc-plus",
    r#"<path d="M9 2.4H4.6c-.7 0-1.2.5-1.2 1.2v8.8c0 .7.5 1.2 1.2 1.2h6.8c.7 0 1.2-.5 1.2-1.2V6z" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" fill="none"/><path d="M8 7.2v4M6 9.2h4" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/>"#
);
icon!(
    DOC_MINUS,
    "doc-minus",
    r#"<path d="M9 2.4H4.6c-.7 0-1.2.5-1.2 1.2v8.8c0 .7.5 1.2 1.2 1.2h6.8c.7 0 1.2-.5 1.2-1.2V6z" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" fill="none"/><path d="M6 9.2h4" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/>"#
);
icon!(
    DOC_ARROW,
    "doc-arrow",
    r#"<path d="M9 2.4H4.6c-.7 0-1.2.5-1.2 1.2v8.8c0 .7.5 1.2 1.2 1.2h6.8c.7 0 1.2-.5 1.2-1.2V6z" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" fill="none"/><path d="M5.8 9.2h4.4M8.6 7.6l1.6 1.6-1.6 1.6" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#
);
icon!(
    ARROW_UP,
    "arrow-up",
    r#"<path d="M8 13V3.4M4 7.2L8 3.2l4 4" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#
);
icon!(STOP, "stop", r#"<rect x="4.4" y="4.4" width="7.2" height="7.2" rx="1.4" fill="currentColor"/>"#);
icon!(
    FOLDER,
    "folder",
    r#"<path d="M2.4 4.4c0-.7.6-1.2 1.2-1.2h2.8l1.4 1.5h4.6c.7 0 1.2.5 1.2 1.2v5.9c0 .7-.5 1.2-1.2 1.2H3.6c-.7 0-1.2-.5-1.2-1.2z" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" fill="none"/>"#
);
icon!(LIST, "list", r#"<path d="M3 4.5h10M3 8h10M3 11.5h6" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"/>"#);
icon!(
    TESTS,
    "tests",
    r#"<rect x="2.5" y="3" width="3" height="3" rx=".8" stroke="currentColor" stroke-width="1.2" fill="none"/><rect x="2.5" y="9.4" width="3" height="3" rx=".8" stroke="currentColor" stroke-width="1.2" fill="none"/><path d="M7.6 4.5h6M7.6 10.9h6" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/>"#
);
icon!(
    PUZZLE,
    "puzzle",
    r#"<path d="M3 5.6h2.4a1.4 1.4 0 1 1 2.8 0H11v2.8a1.4 1.4 0 1 1 0 2.8V13H3v-2.6a1.4 1.4 0 1 0 0-2.8z" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round" fill="none"/>"#
);
icon!(
    CPU,
    "cpu",
    r#"<rect x="4" y="4" width="8" height="8" rx="1.4" stroke="currentColor" stroke-width="1.3" fill="none"/><path d="M6 2v2M10 2v2M6 12v2M10 12v2M2 6h2M2 10h2M12 6h2M12 10h2" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>"#
);
icon!(DOT, "dot", r#"<circle cx="8" cy="8" r="3" fill="currentColor"/>"#);
icon!(
    TERMINAL,
    "terminal",
    r#"<rect x="2" y="3" width="12" height="10" rx="1.8" stroke="currentColor" stroke-width="1.3" fill="none"/><path d="M4.8 6.4l2 1.6-2 1.6M8.4 10h3" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#
);
icon!(
    BRAIN,
    "brain",
    r#"<path d="M6.2 3.2a2 2 0 0 0-2 2 2 2 0 0 0-1.4 3.2 2 2 0 0 0 1.4 3.2 2 2 0 0 0 2 1.6c.8 0 1.4-.4 1.8-1V4.2c-.4-.6-1-1-1.8-1zM9.8 3.2a2 2 0 0 1 2 2 2 2 0 0 1 1.4 3.2 2 2 0 0 1-1.4 3.2 2 2 0 0 1-2 1.6c-.8 0-1.4-.4-1.8-1" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round" fill="none"/>"#
);
icon!(SQUARE, "square", r#"<rect x="3" y="3" width="10" height="10" rx="2.2" stroke="currentColor" stroke-width="1.3" fill="none"/>"#);
icon!(ROUNDED_FILL, "rounded-fill", r#"<rect x="2.6" y="2.6" width="10.8" height="10.8" rx="2.4" fill="currentColor"/>"#);
icon!(PLUS, "plus", r#"<path d="M8 3.5v9M3.5 8h9" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/>"#);
icon!(
    CLOCK,
    "clock",
    r#"<circle cx="8" cy="8" r="5.6" stroke="currentColor" stroke-width="1.3" fill="none"/><path d="M8 5v3.2l2 1.3" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#
);
icon!(
    HELD,
    "held",
    r#"<circle cx="8" cy="8" r="5.6" stroke="currentColor" stroke-width="1.3" fill="none"/><path d="M5.7 6.7h4.6M5.7 9.3h4.6" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/>"#
);
