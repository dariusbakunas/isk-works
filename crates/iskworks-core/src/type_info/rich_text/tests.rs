use super::*;

fn text(text: &str) -> RichSegment {
    RichSegment::Text {
        text: text.to_string(),
        bold: false,
    }
}

fn bold(text: &str) -> RichSegment {
    RichSegment::Text {
        text: text.to_string(),
        bold: true,
    }
}

fn link(type_id: i64, text: &str) -> RichSegment {
    RichSegment::Link {
        type_id,
        text: text.to_string(),
    }
}

#[test]
fn plain_text_is_one_segment() {
    assert_eq!(
        parse_rich_text("bonus to all shield resistances"),
        [text("bonus to all shield resistances")]
    );
}

#[test]
fn empty_text_has_no_segments() {
    assert_eq!(parse_rich_text(""), []);
}

#[test]
fn showinfo_links_keep_the_spaces_around_them() {
    // The Figma prototype ran the link into the next word; the text around
    // a link keeps its own spaces.
    assert_eq!(
        parse_rich_text("10% <a href=showinfo:3454>Microwarpdrive</a> signature radius"),
        [
            text("10% "),
            link(3454, "Microwarpdrive"),
            text(" signature radius"),
        ]
    );
}

#[test]
fn quoted_showinfo_hrefs_are_links_too() {
    assert_eq!(
        parse_rich_text(r#"<a href="showinfo:34">Tritanium</a>"#),
        [link(34, "Tritanium")]
    );
}

#[test]
fn breaks_come_from_br_tags_and_line_breaks() {
    assert_eq!(
        parse_rich_text("one<br>two<br/>three\r\nfour\nfive"),
        [
            text("one"),
            RichSegment::Break,
            text("two"),
            RichSegment::Break,
            text("three"),
            RichSegment::Break,
            text("four"),
            RichSegment::Break,
            text("five"),
        ]
    );
}

#[test]
fn bold_marks_its_text() {
    assert_eq!(
        parse_rich_text("Role Bonus: <b>Can fit</b> things"),
        [text("Role Bonus: "), bold("Can fit"), text(" things")]
    );
}

#[test]
fn other_tags_are_dropped_keeping_their_text() {
    assert_eq!(
        parse_rich_text(r#"<font size="14" color="0xffff0000">Warning</font>: <i>hot</i>"#),
        [text("Warning: hot")]
    );
}

#[test]
fn http_links_become_urls() {
    assert_eq!(
        parse_rich_text(r#"See <a href="https://www.eveonline.com">the site</a>."#),
        [
            text("See "),
            RichSegment::Url {
                href: "https://www.eveonline.com".to_string(),
                text: "the site".to_string(),
            },
            text("."),
        ]
    );
}

#[test]
fn other_link_targets_are_plain_text() {
    // A solar system (`showinfo:5//ID`), a script URL and a missing href.
    assert_eq!(
        parse_rich_text(
            r#"<a href=showinfo:5//30000142>Jita</a> <a href="javascript:alert(1)">x</a> <a>y</a>"#
        ),
        [text("Jita x y")]
    );
}

#[test]
fn entities_are_decoded() {
    assert_eq!(
        parse_rich_text("Fish &amp; chips &lt;3 &quot;q&quot; &#39;s&#39; &apos;a&apos; &gt;"),
        [text("Fish & chips <3 \"q\" 's' 'a' >")]
    );
}

#[test]
fn unknown_entities_stay_as_written() {
    assert_eq!(
        parse_rich_text("a &nbsp b &zz; c"),
        [text("a &nbsp b &zz; c")]
    );
}

#[test]
fn a_link_with_bold_inside_keeps_its_text() {
    assert_eq!(
        parse_rich_text("<a href=showinfo:34><b>Tritanium</b></a>"),
        [link(34, "Tritanium")]
    );
}

#[test]
fn malformed_markup_does_not_panic() {
    for input in [
        "<",
        "<a",
        "<a href=showinfo:",
        "<a href=showinfo:34>never closed",
        "</a></b></font>",
        "a < b > c",
        "<b><b>double</b>",
        "&",
        "&#;",
        "&#99999999999;",
        "<a href=showinfo:99999999999999999999999>big</a>",
        "\u{1F680}<b>\u{00e9}</b>",
    ] {
        let _ = parse_rich_text(input);
    }
    assert_eq!(parse_rich_text("a < b > c"), [text("a < b > c")]);
    assert_eq!(
        parse_rich_text("<a href=showinfo:34>never closed"),
        [link(34, "never closed")]
    );
}

#[test]
fn segments_serialize_with_a_kind_tag() {
    let json = serde_json::to_value(parse_rich_text("x<a href=showinfo:34>T</a><br>")).unwrap();
    assert_eq!(
        json,
        serde_json::json!([
            { "kind": "text", "text": "x", "bold": false },
            { "kind": "link", "typeId": 34, "text": "T" },
            { "kind": "break" },
        ])
    );
}
