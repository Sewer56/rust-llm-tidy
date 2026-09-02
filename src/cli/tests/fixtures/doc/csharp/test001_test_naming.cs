// TEST001 for C#: methods marked with TestMethod, Test, Fact, or Theory
// attributes need behavioral names.
using Microsoft.VisualStudio.TestTools.UnitTesting;

namespace Fixtures;

[TestClass]
/// <summary>Holds the tests.</summary>
public class Tests
{
    [TestMethod]
    public void Test1() { }

    [Test]
    public void Test_foo() { }

    [Fact]
    public void Case_1() { }

    [Theory]
    public void Test() { }

    [TestMethod]
    public void ShouldReturnZeroWhenEmpty() { }
}
