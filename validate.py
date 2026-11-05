#!/usr/bin/env python3
"""Validate the RAEP frame vectors and scenario examples against the schemas.

Usage: pip install jsonschema referencing && python3 validate.py
"""
import json, sys
from jsonschema import Draft202012Validator
from referencing import Registry, Resource
from referencing.jsonschema import DRAFT202012

schemas = json.load(open("schemas.json"))
vectors = json.load(open("vectors.json"))
registry = Registry()
by_id = {}
for s in schemas:
    by_id[s["$id"]] = s
    registry = registry.with_resource(s["$id"], Resource.from_contents(s, default_specification=DRAFT202012))
BASE = "https://raep.vyde.dev/v1/"
frame = Draft202012Validator(by_id[BASE + "frame.json"], registry=registry)
scenario = Draft202012Validator(by_id[BASE + "payloads/conformance_scenario.json"], registry=registry)

failures = 0
for v in vectors["frames"]:
    errors = list(frame.iter_errors(v["frame"]))
    valid = not errors
    if valid != v["valid"]:
        failures += 1
        print("FAIL", v["id"], "expected valid =", v["valid"], [e.message for e in errors][:2])
for s in vectors["scenarios"]:
    errors = list(scenario.iter_errors(s))
    if errors:
        failures += 1
        print("FAIL", s["id"], [e.message for e in errors][:2])
print(len(vectors["frames"]), "frame vectors,", len(vectors["scenarios"]), "scenarios,", failures, "failures")
sys.exit(1 if failures else 0)
