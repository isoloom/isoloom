use proptest::prelude::*;

use super::{Rules, Token, tokens};

fn kept<'a>(rules: &str, paths: &[&'a str]) -> Vec<&'a str> {
    let r = Rules::parse(rules);
    paths.iter().copied().filter(|p| !r.ignored(p)).collect()
}

#[test]
fn gitignore_style_patterns() {
    let paths = [
        "a.txt",
        "logs/x.log",
        "logs/keep.txt",
        "src/app.py",
        "src/deeper/z.py",
        "build/top.o",
        "src/build/out.o",
        "x.tmp",
        "sub/x.tmp",
        ".env",
        "sub/.env",
        "vendor/big/file.bin",
        "vendor/small.txt",
        "data.json",
        "datazjson",
        "docs/sub/deep.md",
        "w/a+b.txt",
        "w/x1",
        "w/x9",
    ];
    let rules = "# not copied\n\nlogs/\n!logs/keep.txt\nbuild/\n/x.tmp\nsub/*.tmp\nvendor/big\n.env\ndocs/**\nsrc/*.py\ndata.js?n\nw/a+b.txt   \r\nw/x[!0-4]\n";
    assert_eq!(
        kept(rules, &paths),
        ["a.txt", "logs/keep.txt", "src/deeper/z.py", "vendor/small.txt", "datazjson", "w/x1"]
    );
}

#[test]
fn folders_only_and_double_stars() {
    // `build/` is a folder only: a file named `build` stays.
    assert_eq!(kept("build/", &["build", "build/", "a/build/x"]), ["build"]);
    assert_eq!(kept("a/**/z", &["a/z", "a/b/z", "a/b/c/z", "b/a/z"]), ["b/a/z"]);
    assert_eq!(kept("**/x", &["x", "a/x", "a/x/y", "ax"]), ["ax"]);
    assert_eq!(kept("a/**", &["a", "a/", "a/b"]), ["a"]);
    assert_eq!(kept("*", &["a", "b/c"]), Vec::<&str>::new());
    assert!(Rules::parse("\n# only comments\n/\n").is_empty());
}

#[test]
fn a_set_takes_a_leading_bracket_and_an_unclosed_one_is_plain() {
    let chars = |s: &str| s.chars().collect::<Vec<char>>();
    assert_eq!(
        tokens(&chars("[]a]")),
        [Token::Set {
            negated: false,
            ranges: vec![(']', ']'), ('a', 'a')]
        }]
    );
    assert_eq!(
        tokens(&chars("[!a-c]x")),
        [
            Token::Set {
                negated: true,
                ranges: vec![('a', 'c')]
            },
            Token::Char('x')
        ]
    );
    assert_eq!(tokens(&chars("[ab")), [Token::Char('['), Token::Char('a'), Token::Char('b')]);
    assert_eq!(tokens(&chars("[]")), [Token::Char('['), Token::Char(']')]);
    assert_eq!(tokens(&chars("a-")), [Token::Char('a'), Token::Char('-')]);
    assert_eq!(kept("[]x]", &["]", "x", "y"]), ["y"]);
    assert_eq!(kept("f[a-]", &["fa", "f-", "fb"]), ["fb"], "a trailing `-` is a member");
}

#[test]
fn only_a_bang_brings_back_and_no_line_is_no_rule() {
    assert!(Rules::parse("a\n!b\n").brings_back());
    assert!(!Rules::parse("a\nb\n").brings_back());
    assert!(Rules::parse("").is_empty() && !Rules::parse("a").is_empty());
    assert!(!Rules::parse("\n  \n# x\n").ignored("a"));
}

/// A name with no glob character.
fn name() -> impl Strategy<Value = String> {
    "[a-z0-9._-]{1,8}"
}

proptest! {
    #[test]
    fn a_plain_name_matches_itself_at_any_depth_and_what_is_in_it(n in name(), d in name()) {
        let r = Rules::parse(&n);
        prop_assert!(r.ignored(&n));
        let nested = format!("{d}/{n}");
        prop_assert!(r.ignored(&nested));
        let inside = format!("{n}/{d}");
        prop_assert!(r.ignored(&inside));
        let longer = format!("{n}x");
        prop_assert!(!r.ignored(&longer));
    }

    #[test]
    fn a_later_bang_brings_a_path_back(n in name(), d in name()) {
        let r = Rules::parse(&format!("{n}\n!{n}\n"));
        prop_assert!(!r.ignored(&n));
        let nested = format!("{d}/{n}");
        prop_assert!(!r.ignored(&nested));
    }

    #[test]
    fn a_folder_pattern_leaves_a_file_of_that_name(n in name(), d in name()) {
        let r = Rules::parse(&format!("{n}/"));
        prop_assert!(!r.ignored(&n));
        let folder = format!("{n}/");
        prop_assert!(r.ignored(&folder));
        let inside = format!("{n}/{d}");
        prop_assert!(r.ignored(&inside));
    }

    #[test]
    fn any_text_parses_and_any_path_gets_the_same_answer_twice(text in "[ -~\n]{0,40}", path in "[ -~]{0,20}") {
        let r = Rules::parse(&text);
        prop_assert_eq!(r.ignored(&path), Rules::parse(&text).ignored(&path));
    }
}
