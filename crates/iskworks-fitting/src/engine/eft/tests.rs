use super::*;

const KNOWN: [&str; 8] = [
    "Rifter",
    "Damage Control II",
    "200mm AutoCannon II",
    "EMP S",
    "Warrior II",
    "Nanite Repair Paste",
    "Gyrostabilizer II",
    "Abyssal Damage Control",
];

fn unknown(text: &str) -> Vec<(usize, &str)> {
    let lines = unknown_lines(text, |name| KNOWN.contains(&name));
    lines
        .into_iter()
        .map(|line| {
            let text = text.lines().nth(line.line - 1).unwrap().trim();
            assert_eq!(line.text, text);
            (line.line, text)
        })
        .collect()
}

#[test]
fn known_modules_charges_stacks_and_empty_slots_pass() {
    let text = "[Rifter, Test]\n\
        Damage Control II /OFFLINE\n\
        [Empty Low slot]\n\
        \n\
        200mm AutoCannon II, EMP S\n\
        \n\
        Warrior II x3\n\
        \n\
        Nanite Repair Paste x100\n";
    assert_eq!(unknown(text), []);
}

#[test]
fn every_unknown_module_charge_and_stack_is_listed() {
    let text = "[Rifter, Test]\n\
        Damage Contrl II\n\
        Gyrostabilizer II\n\
        \n\
        200mm AutoCannon II, EMP M\n\
        \n\
        Warrior I x3\n";
    assert_eq!(
        unknown(text),
        [
            (2, "Damage Contrl II"),
            (5, "200mm AutoCannon II, EMP M"),
            (7, "Warrior I x3")
        ]
    );
}

#[test]
fn an_unknown_ship_is_the_header_line() {
    assert_eq!(
        unknown("\n[Riftr, Test]\nDamage Control II\n"),
        [(2, "[Riftr, Test]")]
    );
}

#[test]
fn mutation_sections_and_references_are_not_type_lines() {
    let text = "[Rifter, Abyssal]\n\
        Damage Control II [1]\n\
        \n\
        [1] Damage Control II\n\
        Unstable Damage Control Mutaplasmid\n\
        Hull Resistance Bonus 30.5, CPU usage 25\n";
    assert_eq!(unknown(text), []);
}

#[test]
fn malformed_text_is_left_for_the_loader() {
    assert_eq!(unknown(""), []);
    assert_eq!(unknown("not eft at all"), []);
}

#[test]
fn state_suffixes_are_lowercased_for_the_loader_in_any_case() {
    assert_eq!(
        lowercase_state_suffixes("Damage Control II /OFFLINE\nWarp Disruptor II/Active"),
        "Damage Control II /offline\nWarp Disruptor II/active"
    );
    assert_eq!(lowercase_state_suffixes("AC/DC Module"), "AC/DC Module");
}

#[test]
fn exports_keep_only_the_offline_suffix_in_the_clients_case() {
    assert_eq!(
        client_state_suffixes(
            "[Rifter, X]\nDamage Control II /offline\n1MN Afterburner II /online\n\
             Warp Disruptor II /active\nGyrostabilizer II\n"
        ),
        "[Rifter, X]\nDamage Control II /OFFLINE\n1MN Afterburner II\nWarp Disruptor II\n\
         Gyrostabilizer II\n"
    );
}
