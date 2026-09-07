//! TEST001 naming lint: marker attributes plus discouraged test names.

use super::{codes, parse};

/// TEST001 fires for every accepted marker attribute on a discouraged
/// name and passes behavioral names.
#[test]
fn test001_flags_discouraged_names_with_any_marker() {
    let source = concat!(
        "/// <summary>Container.</summary>\n",
        "public class C\n",
        "{\n",
        "    [TestMethod]\n",
        "    public void Test1() { }\n",
        "    [Test]\n",
        "    public void Test_foo() { }\n",
        "    [Fact]\n",
        "    public void Case_1() { }\n",
        "    [Theory]\n",
        "    public void Test() { }\n",
        "    [Fact]\n",
        "    public void ShouldReturnZeroWhenEmpty() { }\n",
        "    public void Test_undecorated() { }\n",
        "}\n",
    );
    let parsed = parse(source);
    let found = codes(&parsed, "TEST001");

    // Lines point at each method's attribute line (attributes travel with
    // the declaration, matching the Rust item-line convention).
    assert_eq!(
        found,
        vec!["4:Test1", "6:Test_foo", "8:Case_1", "10:Test"],
        "marker + discouraged name pairs only: {found:?}"
    );
}
