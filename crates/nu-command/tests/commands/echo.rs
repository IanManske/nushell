use nu_test_support::nu;

#[test]
fn echo_range_is_lazy() {
    let actual = nu!("echo 1..10000000000 | first 3 | to json --raw");

    assert_eq!(actual.out, "[1,2,3]");
}

#[test]
fn echo_range_handles_inclusive() {
    let actual = nu!("echo 1..3 | each { |x| $x } | to json --raw");

    assert_eq!(actual.out, "[1,2,3]");
}

#[test]
fn echo_range_handles_exclusive() {
    let actual = nu!("echo 1..<3 | each { |x| $x } | to json --raw");

    assert_eq!(actual.out, "[1,2]");
}

#[test]
fn echo_range_handles_inclusive_down() {
    let actual = nu!("echo 3..1 | each { |it| $it } | to json --raw");

    assert_eq!(actual.out, "[3,2,1]");
}

#[test]
fn echo_range_handles_exclusive_down() {
    let actual = nu!("echo 3..<1 | each { |it| $it } | to json --raw");

    assert_eq!(actual.out, "[3,2]");
}

#[test]
fn echo_is_const() {
    let actual = nu!(r#"const val = echo 1..3; $val | to json --raw"#);
    assert_eq!(actual.out, "[1,2,3]");
}
