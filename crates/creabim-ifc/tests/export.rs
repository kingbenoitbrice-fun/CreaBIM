use creabim_core::sample::sample_house;
use creabim_ifc::{export_ifc4, ExportOptions};

#[test]
fn sample_house_exports_a_complete_ifc4_file() {
    let model = sample_house();
    let ifc = export_ifc4(&model, &ExportOptions::default());
    assert!(ifc.starts_with("ISO-10303-21;"));
    assert!(ifc.contains("FILE_SCHEMA(('IFC4'));"));
    assert!(ifc.trim_end().ends_with("END-ISO-10303-21;"));

    let count = |entity: &str| ifc.matches(&format!("={entity}(")).count();
    assert_eq!(count("IFCPROJECT"), 1);
    assert_eq!(count("IFCBUILDINGSTOREY"), 2);
    assert_eq!(count("IFCWALL"), 9);
    assert_eq!(count("IFCSLAB"), 2);
    assert_eq!(count("IFCDOOR"), 2);
    assert_eq!(count("IFCWINDOW"), 11);
    assert_eq!(count("IFCOPENINGELEMENT"), 13);
    assert_eq!(count("IFCRELVOIDSELEMENT"), 13);
    assert_eq!(count("IFCRELFILLSELEMENT"), 13);
    assert_eq!(count("IFCROOF"), 1);
    assert_eq!(count("IFCMATERIAL"), 4);
    assert_eq!(count("IFCRELASSOCIATESMATERIAL"), 4);

    // Element GlobalIds are the model's own ids, so re-exports keep identity.
    for e in model.iter().filter(|e| e.ifc_class() == "IfcWall") {
        assert!(ifc.contains(&format!("'{}'", e.id.ifc_guid())));
    }
}
