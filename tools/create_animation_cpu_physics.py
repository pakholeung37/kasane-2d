#!/usr/bin/env python3
"""Write self-authored physics3 fixtures for FPS and chained-rig probes."""
import json
from copy import deepcopy
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "tests/fixtures/animation_cpu"
BASE = {
    "Version": 3,
    "Meta": {
        "PhysicsSettingCount": 1,
        "TotalInputCount": 1,
        "TotalOutputCount": 1,
        "VertexCount": 2,
        "EffectiveForces": {"Gravity": {"X": 0, "Y": -1}, "Wind": {"X": 0, "Y": 0}},
        "PhysicsDictionary": [{"Id": "PhysicsProbe", "Name": "Probe"}],
    },
    "PhysicsSettings": [{
        "Id": "PhysicsProbe",
        "Input": [{
            "Source": {"Target": "Parameter", "Id": "ParamX"},
            "Weight": 100, "Type": "X", "Reflect": False,
        }],
        "Output": [{
            "Destination": {"Target": "Parameter", "Id": "ParamY"},
            "VertexIndex": 1, "Scale": 1, "Weight": 100,
            "Type": "Angle", "Reflect": False,
        }],
        "Vertices": [
            {"Position": {"X": 0, "Y": 0}, "Mobility": 1,
             "Delay": 1, "Acceleration": 1, "Radius": 0},
            {"Position": {"X": 0, "Y": 1}, "Mobility": 0.8,
             "Delay": 0.9, "Acceleration": 1.5, "Radius": 1},
        ],
        "Normalization": {
            "Position": {"Minimum": -1, "Default": 0, "Maximum": 1},
            "Angle": {"Minimum": -1, "Default": 0, "Maximum": 1},
        },
    }],
}

if __name__ == "__main__":
    OUT.mkdir(parents=True, exist_ok=True)
    for name, fps in (("missing", None), ("zero", 0), ("thirty", 30)):
        data = deepcopy(BASE)
        if fps is not None:
            data["Meta"]["Fps"] = fps
        path = OUT / f"{name}.physics3.json"
        path.write_text(json.dumps(data, indent=2, ensure_ascii=False) + "\n")
        print(path)
    multi = deepcopy(BASE)
    multi["Meta"]["Fps"] = 30
    multi["Meta"]["PhysicsSettingCount"] = 2
    multi["Meta"]["TotalInputCount"] = 2
    multi["Meta"]["TotalOutputCount"] = 2
    multi["Meta"]["VertexCount"] = 4
    multi["Meta"]["PhysicsDictionary"].append({"Id": "PhysicsChained", "Name": "Chained"})
    second = deepcopy(BASE["PhysicsSettings"][0])
    second["Id"] = "PhysicsChained"
    second["Input"][0]["Source"]["Id"] = "ParamY"
    second["Input"][0]["Type"] = "Angle"
    second["Input"][0]["Reflect"] = True
    second["Output"][0]["Weight"] = 60
    multi["PhysicsSettings"].append(second)
    path = OUT / "multi.physics3.json"
    path.write_text(json.dumps(multi, indent=2, ensure_ascii=False) + "\n")
    print(path)
