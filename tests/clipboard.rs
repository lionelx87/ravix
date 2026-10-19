use ravix::clipboard::osc52;

#[test]
fn osc52_wraps_the_text_in_base64_for_the_clipboard_selection() {
    assert_eq!(osc52("hello"), "\x1b]52;c;aGVsbG8=\x07");
}

#[test]
fn osc52_pads_every_remainder_length() {
    assert_eq!(osc52("ab"), "\x1b]52;c;YWI=\x07");
    assert_eq!(osc52("abc"), "\x1b]52;c;YWJj\x07");
    assert_eq!(osc52("a"), "\x1b]52;c;YQ==\x07");
}
