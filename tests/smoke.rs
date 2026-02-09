use std::path::PathBuf;
use graze::provider::create_provider;

#[test]
fn load_csv_and_fetch_pages() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-100k.csv");
    if !path.exists() {
        eprintln!("skipping: test-100k.csv not found");
        return;
    }

    let provider = create_provider(&path).expect("create_provider failed");

    assert_eq!(provider.total_rows(), 100_000);
    assert_eq!(provider.name(), "CSV");

    let schema = provider.schema();
    let fields: Vec<&str> = schema.fields().iter().map(|f| f.name().as_str()).collect();
    assert_eq!(fields, &["id", "first name", "last_name", "age", "city", "salary", "active"]);

    // Fetch first page
    let batch = provider.fetch_page(0, 50).expect("fetch_page 0..50");
    assert_eq!(batch.num_rows(), 50);
    assert_eq!(batch.num_columns(), 7);

    // Fetch a middle page
    let batch = provider.fetch_page(50_000, 100).expect("fetch_page 50000..50100");
    assert_eq!(batch.num_rows(), 100);

    // Fetch page at the end
    let batch = provider.fetch_page(99_990, 100).expect("fetch_page 99990..100090");
    assert_eq!(batch.num_rows(), 10); // only 10 rows left

    // Fetch beyond the end
    let batch = provider.fetch_page(200_000, 50).expect("fetch_page beyond end");
    assert_eq!(batch.num_rows(), 0);

    // Large buffer fetch (simulating 5x page_size)
    let batch = provider.fetch_page(0, 250).expect("fetch_page 0..250");
    assert_eq!(batch.num_rows(), 250);

    println!("All page fetches OK");
}

#[test]
fn sort_and_filter() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-100k.csv");
    if !path.exists() {
        return;
    }

    let mut provider = create_provider(&path).expect("create_provider failed");

    // Sort by age ascending
    let mut sort_state = graze::event::SortState::default();
    sort_state.toggle("age");
    provider.apply_sort_state(&sort_state).expect("sort");
    let batch = provider.fetch_page(0, 10).expect("sorted page");
    assert_eq!(batch.num_rows(), 10);

    // Filter: age > 60
    let count = provider.apply_filter("age > 60").expect("filter");
    assert!(count > 0 && count < 100_000, "filter count {count} should be between 0 and 100000");

    let batch = provider.fetch_page(0, count).expect("filtered page");
    assert_eq!(batch.num_rows(), count);

    // Reset
    let total = provider.reset_filters().expect("reset");
    assert_eq!(total, 100_000);

    println!("Sort and filter OK");
}
