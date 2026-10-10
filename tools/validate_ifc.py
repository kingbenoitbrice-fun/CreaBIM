"""Validate a CreaBIM IFC export with IfcOpenShell.

Checks the file against the IFC schema, then builds geometry for every
product to prove other software can read it. Exits non-zero on any problem.

    python3 -m pip install ifcopenshell
    python3 tools/validate_ifc.py house.ifc
"""

import logging
import sys

import ifcopenshell
import ifcopenshell.geom
import ifcopenshell.validate


def main(path: str) -> int:
    f = ifcopenshell.open(path)
    print(f"schema {f.schema}, {len(list(f))} entities")

    logger = ifcopenshell.validate.json_logger()
    ifcopenshell.validate.validate(f, logger, express_rules=True)
    for s in logger.statements:
        print("SCHEMA:", s.get("message"), s.get("instance"))
    problems = len(logger.statements)

    settings = ifcopenshell.geom.settings()
    for p in f.by_type("IfcProduct"):
        if p.Representation is None or p.is_a("IfcOpeningElement"):
            continue
        try:
            shape = ifcopenshell.geom.create_shape(settings, p)
            n = len(shape.geometry.faces) // 3
            print(f"  {p.is_a():24} {p.Name:24} {n:5} triangles")
            if n == 0:
                problems += 1
        except Exception as e:  # noqa: BLE001 - report every failure
            print(f"GEOMETRY: {p.is_a()} {p.Name}: {e}")
            problems += 1

    storeys = f.by_type("IfcBuildingStorey")
    contained = sum(len(r.RelatedElements) for r in f.by_type("IfcRelContainedInSpatialStructure"))
    voids = len(f.by_type("IfcRelVoidsElement"))
    print(f"{len(storeys)} storeys, {contained} contained elements, {voids} openings")
    print("OK" if problems == 0 else f"{problems} problem(s)")
    return 0 if problems == 0 else 1


if __name__ == "__main__":
    logging.disable(logging.WARNING)
    sys.exit(main(sys.argv[1]))
