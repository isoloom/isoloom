use proptest::prelude::*;

use super::{Call, Provision, Ruby, Start, Value, closing_tag, quote};

#[test]
fn quotes_backslashes_and_interpolations_are_escaped() {
    assert_eq!(quote(r#"a"b\c"#), r#""a\"b\\c""#);
    assert_eq!(quote("#{x} #$y #@z # {"), r#""\#{x} \#$y \#@z # {""#);
    // A string value is quoted so; a raw one is Ruby as written.
    assert_eq!(Value::str("a \"b\" #{c}").to_string(), r#""a \"b\" \#{c}""#);
    assert_eq!(Value::raw("ENV[\"X\"]").to_string(), "ENV[\"X\"]");
}

#[test]
fn a_vagrantfile_is_written_from_typed_pieces() {
    let mut r = Ruby::new();
    r.block("Vagrant.configure(\"2\")", "config", |r| {
        r.assign("config.vm.box", Value::str("bento/debian-12"));
        r.block(Call::new("config.vm.provider").arg(Value::str("virtualbox")), "v", |r| {
            r.assign("v.cpus", 2u32);
        });
        r.provision(Provision::new("config", "shell").kw("inline", Value::str("echo hi")));
    });
    assert_eq!(
        r.finish(),
        "Vagrant.configure(\"2\") do |config|\n  config.vm.box = \"bento/debian-12\"\n  config.vm.provider \"virtualbox\" do |v|\n    v.cpus = 2\n  end\n  config.vm.provision \"shell\", inline: \"echo hi\"\nend\n"
    );
}

#[test]
fn a_heredoc_tag_never_is_a_line_of_its_script() {
    assert_eq!(closing_tag("SH", "a\nb"), "SH");
    assert_eq!(closing_tag("SH", "a\n  SH\nb"), "ISOLOOM_SH");
    assert_eq!(closing_tag("SH", "SH\nISOLOOM_SH\nISOLOOM_SH_2"), "ISOLOOM_SH_3");
    let mut r = Ruby::new();
    r.heredoc_argument("f(", ")", "PS1", "PS1\nx");
    assert_eq!(r.finish(), "f(<<~'ISOLOOM_PS1')\n  PS1\n  x\nISOLOOM_PS1\n");
}

proptest! {
    #[test]
    fn a_heredoc_ends_only_at_its_own_closing_line(lines in prop::collection::vec("( *)(SH|ISOLOOM_SH|ISOLOOM_SH_2|[a-z]{1,6})", 2..8)) {
        let script = lines.join("\n");
        let mut r = Ruby::new();
        r.provision(Provision::new("p", "shell").script("inline", &script));
        let written = r.finish();
        let rows: Vec<&str> = written.lines().collect();
        let (first, rest) = rows.split_first().expect("the call");
        let tag = first.split("<<~'").nth(1).and_then(|t| t.strip_suffix('\'')).expect("a heredoc");
        let (last, body) = rest.split_last().expect("the closing line");
        prop_assert_eq!(*last, tag);
        for line in body {
            prop_assert_ne!(line.trim(), tag, "{}", written);
        }
        prop_assert_eq!(body.len(), lines.len());
    }
}

#[test]
fn values_compose() {
    assert_eq!(Value::sym("windows").to_string(), ":windows");
    assert_eq!(Value::array([Value::str("a"), 2u16.into()]).to_string(), "[\"a\", 2]");
    assert_eq!(Value::hash([(Value::str("k"), true.into())]).to_string(), "{ \"k\" => true }");
    assert_eq!(Value::hash([]).to_string(), "{}");
}

#[test]
fn a_call_lists_positional_then_keyword_arguments() {
    let c = Call::new("m.vm.network")
        .arg(Value::str("forwarded_port"))
        .kw("guest", 80u16)
        .kw_opt("protocol", None)
        .kw_opt("id", Some(Value::str("web")))
        .comment("why");
    assert_eq!(c.to_string(), "m.vm.network \"forwarded_port\", guest: 80, id: \"web\" # why");
}

#[test]
fn blocks_nest_and_indent() {
    let mut r = Ruby::new();
    r.block("a", "", |r| {
        r.block(Call::new("b").arg(Value::str("c")), "v, o", |r| {
            r.assign_if("v.x", Value::raw("ENV[\"X\"]"), "ENV[\"X\"]");
        });
    });
    assert_eq!(r.finish(), "a do\n  b \"c\" do |v, o|\n    v.x = ENV[\"X\"] if ENV[\"X\"]\n  end\nend\n");
}

#[test]
fn a_heredoc_indents_its_script_and_keeps_blank_lines_empty() {
    let mut r = Ruby::new();
    r.provision(Provision::new("m", "shell").heredoc("inline", "SH", "set -e\n\necho hi\n"));
    assert_eq!(r.finish(), "m.vm.provision \"shell\", inline: <<~'SH'\n  set -e\n\n  echo hi\nSH\n");
}

#[test]
fn a_script_is_a_string_on_one_line_and_a_heredoc_on_more() {
    let mut r = Ruby::new();
    let p = || Provision::new("p", "shell");
    r.provision(p().script("inline", "echo \"hi\"\n"));
    r.provision(p().script("inline", "a\nb\n"));
    r.provision(p().script("inline", "cat <<SH\nx\nSH\n"));
    assert_eq!(
        r.finish(),
        "p.vm.provision \"shell\", inline: \"echo \\\"hi\\\"\"\np.vm.provision \"shell\", inline: <<~'SH'\n  a\n  b\nSH\np.vm.provision \"shell\", inline: <<~'ISOLOOM_SH'\n  cat <<SH\n  x\n  SH\nISOLOOM_SH\n"
    );
}

#[test]
fn an_if_block_nests() {
    let mut r = Ruby::new();
    r.if_block("a != b", |r| {
        r.assign("x", 1u32);
    });
    assert_eq!(r.finish(), "if a != b\n  x = 1\nend\n");
}

#[test]
fn a_hash_constant_lists_one_entry_a_line() {
    let mut r = Ruby::new();
    r.comment("Inputs.").hash("INPUTS", [(Value::str("A"), Value::raw("ENV.fetch(\"A\", \"\")"))]);
    assert_eq!(r.finish(), "# Inputs.\nINPUTS = {\n  \"A\" => ENV.fetch(\"A\", \"\"),\n}\n");
}

#[test]
fn text_goes_in_as_it_is() {
    let mut r = Ruby::new();
    r.text("x\n").blank();
    assert_eq!(r.finish(), "x\n\n");
}

#[test]
fn an_on_demand_define_is_not_started_by_up() {
    let mut r = Ruby::new();
    r.assign("ROOT", Value::raw("1"));
    r.configure(|r| {
        r.assign("config.vm.boot_timeout", 900u32);
        r.comment("shared");
        r.provision(Provision::new("config", "shell").heredoc("inline", "SH", "echo a\n"));
        r.define("web", Start::OnDemand, |r| {
            r.block(Call::new("m.vm.provider").arg(Value::str("utm")), "_, o", |r| {
                r.assign("o.memory", 512u32);
            });
            r.provision(Provision::new("m", "shell").script("inline", "b"));
        });
    });
    let file = r.finish();
    let defines = isoloom_test_support::ruby_blocks(&file)
        .find("config.vm.define ")
        .into_iter()
        .map(|d| d.head.clone())
        .collect::<Vec<_>>();
    assert_eq!(defines, ["config.vm.define \"web\", autostart: false do |m|"], "{file}");
}
