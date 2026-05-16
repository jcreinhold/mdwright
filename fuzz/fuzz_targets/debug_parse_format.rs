#![no_main]
use libfuzzer_sys::fuzz_target;
use mdwright::{Document, FmtOptions, render_html};

fuzz_target!(|data: &[u8]| {
    if data.len() > 65536 {
        return;
    }
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    if s != "<?" {
        return;
    }
    eprintln!("=== Testing: {:?}", s);
    eprintln!("=== Input bytes: {:?}", s.as_bytes());
    
    let before = render_html(s);
    eprintln!("=== Before: {:?}", before);
    eprintln!("=== Before bytes: {:?}", before.as_bytes());
    
    let formatted = Document::parse(s).format(&FmtOptions::default());
    eprintln!("=== Formatted: {:?}", formatted);
    eprintln!("=== Formatted bytes: {:?}", formatted.as_bytes());
    
    let after = render_html(&formatted);
    eprintln!("=== After: {:?}", after);
    eprintln!("=== After bytes: {:?}", after.as_bytes());
    
    assert_eq!(before, after, "format changes HTML meaning: before={:?} after={:?}", before, after);
});
