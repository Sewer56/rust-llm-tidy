//! Benchmark fixture: Reloaded Search Engine search/filter logic (leptos/WASM).
//! 506 lines, lint-clean.
//! Source: crate `reloaded-search-engine` by Sewer56; no public remote appears to exist.
//! Embedded verbatim via include_str! in benches/common.rs.
//! Search functionality and filtering logic

use crate::data::{Package, PackageRegistry};
use leptos::{Signal, SignalGet};

/// Creates a reactive signal for filtered packages
///
/// This function creates a derived signal that automatically updates
/// when either the package registry or search query changes.
///
/// # Arguments
/// * `registry` - Signal containing the package registry
/// * `query` - Signal containing the search query
///
/// # Returns
/// A signal containing the filtered packages
pub fn use_filtered_packages(
    registry: Signal<Option<PackageRegistry>>,
    query: Signal<String>,
) -> Signal<Vec<Package>> {
    Signal::derive(move || {
        let reg_option = registry.get();
        let q = query.get();

        log::debug!(
            "🔄 use_filtered_packages reacting: registry_available={}, query='{}'",
            reg_option.is_some(),
            q
        );

        if let Some(reg) = reg_option {
            let total_packages = reg.packages.len();
            log::debug!(
                "📦 Registry available with {} packages, filtering...",
                total_packages
            );
            let results = filter_packages(&reg, &q);
            log::debug!("📊 Filtered to {} results", results.len());
            results
        } else {
            log::debug!("📦 No registry available, returning empty results");
            Vec::new()
        }
    })
}

/// Filters packages based on search query
///
/// This function searches through package names, descriptions, authors, and modIds
/// to find matches for the given query. The search is case-insensitive
/// and matches partial strings.
///
/// # Arguments
/// * `registry` - The package registry containing all packages
/// * `query` - The search query string
///
/// # Returns
/// A vector of packages that match the search criteria
pub fn filter_packages(registry: &PackageRegistry, query: &str) -> Vec<Package> {
    log::debug!("🔍 filter_packages called with query: '{}'", query);

    if query.trim().is_empty() {
        let all_packages: Vec<_> = registry.packages.values().cloned().collect();
        log::debug!(
            "📋 Empty query, returning all {} packages",
            all_packages.len()
        );
        return all_packages;
    }

    let normalized_query = query.trim().to_lowercase();
    let mut filtered_packages = Vec::new();

    log::debug!(
        "🔍 Searching through {} packages for '{}'",
        registry.packages.len(),
        normalized_query
    );

    for (mod_id, package) in &registry.packages {
        // Check if query matches name, description, authors, or modId
        let name_matches = package.name.to_lowercase().contains(&normalized_query);
        let description_matches = package
            .description
            .to_lowercase()
            .contains(&normalized_query);
        let authors_match = package
            .authors
            .iter()
            .any(|author| author.to_lowercase().contains(&normalized_query));
        let mod_id_matches = mod_id.to_lowercase().contains(&normalized_query);

        let matches = name_matches || description_matches || authors_match || mod_id_matches;

        if matches {
            filtered_packages.push(package.clone());

            // Track match types for debugging
            let match_types = vec![
                (name_matches, "name"),
                (description_matches, "description"),
                (authors_match, "authors"),
                (mod_id_matches, "modId"),
            ]
            .into_iter()
            .filter_map(|(matches, field)| if matches { Some(field) } else { None })
            .collect::<Vec<_>>();

            log::debug!(
                "  ✅ Match found: '{}' matches in {:?}",
                package.name,
                match_types
            );
        }
    }

    log::debug!(
        "📊 Found {} matching packages before sorting",
        filtered_packages.len()
    );

    // Sort by relevance: name/modId matches first, then description matches
    filtered_packages.sort_by(|a, b| {
        let a_name_score = if a.name.to_lowercase().contains(&normalized_query) {
            3
        } else {
            0
        };
        let b_name_score = if b.name.to_lowercase().contains(&normalized_query) {
            3
        } else {
            0
        };

        let a_desc_score = if a.description.to_lowercase().contains(&normalized_query) {
            1
        } else {
            0
        };
        let b_desc_score = if b.description.to_lowercase().contains(&normalized_query) {
            1
        } else {
            0
        };

        // Find the modId for each package by looking up in the registry
        let a_mod_id = registry
            .packages
            .iter()
            .find(|(_, pkg)| pkg.name == a.name)
            .map(|(mod_id, _)| mod_id);
        let b_mod_id = registry
            .packages
            .iter()
            .find(|(_, pkg)| pkg.name == b.name)
            .map(|(mod_id, _)| mod_id);

        let a_mod_id_score = if let Some(mod_id) = a_mod_id {
            if mod_id.to_lowercase().contains(&normalized_query) {
                3 // Same priority as name matches
            } else {
                0
            }
        } else {
            0
        };

        let b_mod_id_score = if let Some(mod_id) = b_mod_id {
            if mod_id.to_lowercase().contains(&normalized_query) {
                3 // Same priority as name matches
            } else {
                0
            }
        } else {
            0
        };

        let a_score = a_name_score + a_mod_id_score + a_desc_score;
        let b_score = b_name_score + b_mod_id_score + b_desc_score;

        // Higher score first, then by download count as secondary sort
        b_score
            .cmp(&a_score)
            .then_with(|| b.download_count.cmp(&a.download_count))
    });

    log::info!(
        "🎯 filter_packages returning {} packages for query '{}'",
        filtered_packages.len(),
        query
    );
    filtered_packages
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_package(name: &str, description: &str, authors: Vec<&str>) -> Package {
        Package {
            name: name.to_string(),
            authors: authors.into_iter().map(|s| s.to_string()).collect(),
            description: description.to_string(),
            images: vec![],
            file_size: 1000,
            download_count: 100,
            view_count: 200,
            like_count: 50,
            published: "2023-01-01T00:00:00Z".to_string(),
            project_uri: "https://example.com".to_string(),
        }
    }

    #[test]
    fn test_filter_packages_empty_query() {
        let mut registry = PackageRegistry::new();
        registry.packages.insert(
            "test_mod".to_string(),
            create_test_package("Test Mod", "A test mod", vec!["Author1"]),
        );

        let results = filter_packages(&registry, "");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_filter_packages_name_match() {
        let mut registry = PackageRegistry::new();
        registry.packages.insert(
            "test_mod".to_string(),
            create_test_package("Test Mod", "A test mod", vec!["Author1"]),
        );
        registry.packages.insert(
            "other_mod".to_string(),
            create_test_package("Other Mod", "Another mod", vec!["Author2"]),
        );

        let results = filter_packages(&registry, "test");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Test Mod");
    }

    #[test]
    fn test_filter_packages_description_match() {
        let mut registry = PackageRegistry::new();
        registry.packages.insert(
            "test_mod".to_string(),
            create_test_package("Test Mod", "A wonderful test mod", vec!["Author1"]),
        );
        registry.packages.insert(
            "other_mod".to_string(),
            create_test_package("Other Mod", "Another mod", vec!["Author2"]),
        );

        let results = filter_packages(&registry, "wonderful");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Test Mod");
    }

    #[test]
    fn test_filter_packages_author_match() {
        let mut registry = PackageRegistry::new();
        registry.packages.insert(
            "test_mod".to_string(),
            create_test_package("Test Mod", "A test mod", vec!["TestAuthor"]),
        );
        registry.packages.insert(
            "other_mod".to_string(),
            create_test_package("Other Mod", "Another mod", vec!["OtherAuthor"]),
        );

        let results = filter_packages(&registry, "testauthor");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Test Mod");
    }

    #[test]
    fn test_filter_packages_case_insensitive() {
        let mut registry = PackageRegistry::new();
        registry.packages.insert(
            "test_mod".to_string(),
            create_test_package("Test Mod", "A test mod", vec!["Author1"]),
        );

        let results = filter_packages(&registry, "TEST");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_filter_packages_no_matches() {
        let mut registry = PackageRegistry::new();
        registry.packages.insert(
            "test_mod".to_string(),
            create_test_package("Test Mod", "A test mod", vec!["Author1"]),
        );

        let results = filter_packages(&registry, "nonexistent");
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_filter_packages_mod_id_search() {
        let mut registry = PackageRegistry::new();
        registry.packages.insert(
            "com.example.testmod".to_string(),
            create_test_package("Test Mod", "A test mod", vec!["Author1"]),
        );
        registry.packages.insert(
            "com.example.othermod".to_string(),
            create_test_package("Other Mod", "Another mod", vec!["Author2"]),
        );

        // Search by modId
        let results = filter_packages(&registry, "com.example.testmod");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Test Mod");

        // Partial modId search
        let results = filter_packages(&registry, "example");
        assert_eq!(results.len(), 2); // Should match both mods
    }

    #[test]
    fn test_filter_packages_relevance_sorting() {
        let mut registry = PackageRegistry::new();

        // Create packages with different download counts for testing sorting
        let mut package1 = create_test_package("Test Mod", "A test mod", vec!["Author1"]);
        package1.download_count = 1000;

        let mut package2 = create_test_package("Test", "Another test mod", vec!["Author2"]);
        package2.download_count = 500;

        let mut package3 = create_test_package("Other Mod", "Test description", vec!["Author3"]);
        package3.download_count = 2000;

        registry.packages.insert("test_mod".to_string(), package1);
        registry.packages.insert("test".to_string(), package2);
        registry.packages.insert("other_mod".to_string(), package3);

        let results = filter_packages(&registry, "test");
        assert_eq!(results.len(), 3);

        // Name matches should come first (higher score), sorted by download count within name matches
        assert_eq!(results[0].name, "Test Mod"); // name match, 1000 downloads
        assert_eq!(results[1].name, "Test"); // name match, 500 downloads
        assert_eq!(results[2].name, "Other Mod"); // description match, 2000 downloads
    }

    #[test]
    fn test_filter_packages_whitespace_handling() {
        let mut registry = PackageRegistry::new();
        registry.packages.insert(
            "test_mod".to_string(),
            create_test_package("Test Mod", "A test mod", vec!["Author1"]),
        );

        // Test with leading/trailing whitespace
        let results = filter_packages(&registry, "  test  ");
        assert_eq!(results.len(), 1);

        // Test with only whitespace
        let results = filter_packages(&registry, "   ");
        assert_eq!(results.len(), 1); // Should return all packages
    }

    #[test]
    fn test_filter_packages_mod_id_relevance_sorting() {
        let mut registry = PackageRegistry::new();

        // Create packages with different download counts for testing modId relevance sorting
        let mut package1 = create_test_package("Mod One", "A test mod", vec!["Author1"]);
        package1.download_count = 500;

        let mut package2 = create_test_package("Mod Two", "Another test mod", vec!["Author2"]);
        package2.download_count = 1000;

        let mut package3 = create_test_package("Mod Three", "Test description", vec!["Author3"]);
        package3.download_count = 2000;

        // Insert with specific modIds
        registry
            .packages
            .insert("com.example.testmod".to_string(), package1);
        registry.packages.insert("other.mod".to_string(), package2);
        registry.packages.insert("third.mod".to_string(), package3);

        // Search for "testmod" - should match modId exactly
        let results = filter_packages(&registry, "testmod");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Mod One"); // Only the one with matching modId

        // Search for "example" - should match modId partially
        let results = filter_packages(&registry, "example");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Mod One");
    }

    #[test]
    fn test_filter_packages_mod_id_case_sensitivity() {
        let mut registry = PackageRegistry::new();
        registry.packages.insert(
            "Com.Example.TestMod".to_string(),
            create_test_package("Test Mod", "A test mod", vec!["Author1"]),
        );
        registry.packages.insert(
            "com.example.othermod".to_string(),
            create_test_package("Other Mod", "Another mod", vec!["Author2"]),
        );

        // Test case insensitive modId search
        let results = filter_packages(&registry, "COM.EXAMPLE.TESTMOD");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Test Mod");

        let results = filter_packages(&registry, "com.example.testmod");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Test Mod");

        let results = filter_packages(&registry, "example");
        assert_eq!(results.len(), 2); // Should match both mods
    }

    #[test]
    fn test_filter_packages_mod_id_exact_vs_partial_priority() {
        let mut registry = PackageRegistry::new();

        let mut package1 = create_test_package("Exact Match", "A test mod", vec!["Author1"]);
        package1.download_count = 100;

        let mut package2 =
            create_test_package("Partial Match", "Another test mod", vec!["Author2"]);
        package2.download_count = 1000;

        // Insert packages with different download counts
        registry.packages.insert("testmod".to_string(), package1); // Exact modId match
        registry
            .packages
            .insert("some.testmod.package".to_string(), package2); // Partial modId match

        let results = filter_packages(&registry, "testmod");
        assert_eq!(results.len(), 2);

        // Both should have same relevance score (3 points each), sorted by download count
        assert_eq!(results[0].name, "Partial Match"); // 1000 downloads
        assert_eq!(results[1].name, "Exact Match"); // 100 downloads
    }

    #[test]
    fn test_filter_packages_mod_id_vs_name_priority() {
        let mut registry = PackageRegistry::new();

        let mut package1 = create_test_package("TestMod", "A test mod", vec!["Author1"]);
        package1.download_count = 1000;

        let mut package2 = create_test_package("OtherMod", "Another test mod", vec!["Author2"]);
        package2.download_count = 2000;

        // Insert packages where one matches by name, other by modId
        registry.packages.insert("other.mod".to_string(), package1); // Name matches "TestMod"
        registry.packages.insert("testmod".to_string(), package2); // modId matches "testmod"

        let results = filter_packages(&registry, "testmod");
        assert_eq!(results.len(), 2);

        // Both should have same relevance score (3 points each), sorted by download count
        assert_eq!(results[0].name, "OtherMod"); // 2000 downloads (name match)
        assert_eq!(results[1].name, "TestMod"); // 1000 downloads (modId match)
    }

    #[test]
    fn test_filter_packages_mod_id_complex_scenarios() {
        let mut registry = PackageRegistry::new();

        let mut package1 = create_test_package("Game Mod", "A gaming mod", vec!["Author1"]);
        package1.download_count = 1500;

        let mut package2 = create_test_package(
            "Utility Mod",
            "A utility mod with testmod in description",
            vec!["Author2"],
        );
        package2.download_count = 1000;

        let mut package3 = create_test_package("Another Mod", "Just another mod", vec!["Author3"]);
        package3.download_count = 500;

        // Insert packages
        registry
            .packages
            .insert("com.game.testmod".to_string(), package1); // modId match
        registry
            .packages
            .insert("com.utility.mod".to_string(), package2); // description match
        registry
            .packages
            .insert("com.another.mod".to_string(), package3); // no match

        let results = filter_packages(&registry, "testmod");
        assert_eq!(results.len(), 2);

        // First should be modId match (3 points), second should be description match (1 point)
        assert_eq!(results[0].name, "Game Mod"); // modId match, higher relevance
        assert_eq!(results[1].name, "Utility Mod"); // description match, lower relevance
    }
}
