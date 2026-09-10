// TEST002 for C#: test methods need a summary comment above their attributes.
// A `///` XML doc run or a plain `//` block directly above the attributes
// satisfies the rule; presence only is checked, never wording.
//
// Expected diagnostics:
// - TEST002 on `MissingSummary` (no comment above the attributes)
// - TEST002 on `BlankLineBeforeAttributes` (comment separated by a blank line)
// - TEST002 on `CommentBelowAttributes` (comment below the attribute block)
// - TEST002 on `IgnoredWithoutSummary` (`[Ignore]` grants no exemption)
//
// Not flagged (should pass):
// - `DocCommentSummary` (`///` above the attributes)
// - `PlainCommentSummary` (`//` above the attributes)
// - `NotATest` (no test marker)

public class SummaryTests
{
    [TestMethod]
    public void MissingSummary() { }

    // This comment is separated from the attribute block by a blank line.

    [TestMethod]
    public void BlankLineBeforeAttributes() { }

    [TestMethod]
    // A comment below the attributes does not count as a summary.
    public void CommentBelowAttributes() { }

    [Ignore]
    [TestMethod]
    public void IgnoredWithoutSummary() { }

    /// <summary>Verifies the XML doc path.</summary>
    [TestMethod]
    public void DocCommentSummary() { }

    // Verifies the plain-comment path.
    [TestMethod]
    public void PlainCommentSummary() { }

    public void NotATest() { }
}
