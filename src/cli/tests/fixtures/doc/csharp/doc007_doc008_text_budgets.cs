// C# text checks fire on over-budget doc prose: DOC007 errors on a
// summary paragraph over 240 measured chars, DOC008 warns on a line
// whose tag-stripped inner text exceeds 80 chars.
namespace Fixtures;

/// <summary>Budget probes.</summary>
public class Budgets
{
    /// <summary>
    /// Computes the frobnication index for the input sequence by walking
    /// every element, applying the discount table, folding in the rebate
    /// rules, clamping the running total, and rounding half-up at the end
    /// before returning the final integer index for the caller to use.
    /// </summary>
    /// <param name="input">The input sequence.</param>
    public int IndexOf(string input) { return input.Length; }

    /// <summary>Describes the parameter.</summary>
    /// <param name="input">The input sequence whose frobnication index this method computes, discounts, and returns as an integer.</param>
    public int Describe(string input) { return input.Length; }
}
