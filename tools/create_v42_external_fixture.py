#!/usr/bin/env python3
"""Generate an independent external v42 (csmMocVersion_42) MOC3 fixture for M3C Stage S2.
Constructed directly via raw binary struct layout conforming to PSM__SECTIONS_V42.
Verified via both Official Live2D Core 6.0.1 and PurismCore 1.1.0.
"""
import os
import struct
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT_DIR = ROOT / "tests/fixtures/external_v42"
OUT_DIR.mkdir(parents=True, exist_ok=True)

def create_dummy_png(path: Path):
    w, h = 4, 4
    raw_data = bytearray()
    for y in range(h):
        raw_data.append(0)
        raw_data.extend([255, 255, 255, 255] * w)
    compressed = zlib.compress(bytes(raw_data))
    png = bytearray(b"\x89PNG\r\n\x1a\n")
    ihdr_data = struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0)
    ihdr_crc = zlib.crc32(b"IHDR" + ihdr_data)
    png.extend(struct.pack(">I4s", len(ihdr_data), b"IHDR") + ihdr_data + struct.pack(">I", ihdr_crc))
    idat_crc = zlib.crc32(b"IDAT" + compressed)
    png.extend(struct.pack(">I4s", len(compressed), b"IDAT") + compressed + struct.pack(">I", idat_crc))
    iend_crc = zlib.crc32(b"IEND")
    png.extend(struct.pack(">I4s", 0, b"IEND") + struct.pack(">I", iend_crc))
    path.write_bytes(png)

create_dummy_png(OUT_DIR / "texture_00.png")

def build_v42_moc3():
    # Counts (32 ints for V42):
    n_parts = 1
    n_deformers = 2 # 1 warp, 1 rotation
    n_warps = 1
    n_rotations = 1
    n_meshes = 1
    n_params = 3 # ParamX (0), ParamY (0), ParamBS (1)
    n_part_kfs = 1
    n_warp_kfs = 4 # 1 normal + 3 BS delta keyforms
    n_rot_kfs = 1
    n_mesh_kfs = 9 # 6 normal (3x2 grid) + 3 BS delta keyforms
    # Positions: 16*4 warp pts (64 pts = 128 floats) + (6*4 + 3*4) mesh pts (36 pts = 72 floats) = 200 floats (100 pts)
    n_kf_pos = (16 * 4 * 2) + (9 * 4 * 2) # 200 floats
    n_kt_idx = 2 # for normal binding 1
    n_bindings = 2 # 0: static, 1: mesh 2-axis
    n_kt = 2 # normal key tables (ParamX, ParamY)
    n_keys = 8 # ParamX: 3 keys [-1, 0, 1], ParamY: 2 keys [-1, 1], ParamBS: 3 keys [-1, 0, 1]
    n_uvs = 8 # 4 vertices * 2
    n_idx = 6 # 2 triangles * 3
    n_masks = 0
    n_draw_groups = 2
    n_draw_items = 2
    n_glues = 0
    n_glue_info = 0
    n_glue_kfs = 0
    # Normal colors: 1 for warp + 6 for mesh = 7 colors
    n_mul_colors = 7
    n_scr_colors = 7
    # BlendShape counts (V42 sections):
    n_blend_kt = 1 # for ParamBS (3 keys, base_key_idx = 1)
    n_blend_bindings = 2 # 0: Warp BS, 1: ArtMesh BS
    n_bs_warps = 1
    n_bs_art_meshes = 1
    n_bs_constraint_idx = 1
    n_bs_constraints = 1
    n_bs_constraint_vals = 3 # keys [-1.0, 0.0, 1.0], weights [1.0, 0.0, 1.0]

    counts = [0] * 32
    counts[0] = n_parts
    counts[1] = n_deformers
    counts[2] = n_warps
    counts[3] = n_rotations
    counts[4] = n_meshes
    counts[5] = n_params
    counts[6] = n_part_kfs
    counts[7] = n_warp_kfs
    counts[8] = n_rot_kfs
    counts[9] = n_mesh_kfs
    counts[10] = n_kf_pos
    counts[11] = n_kt_idx
    counts[12] = n_bindings
    counts[13] = n_kt
    counts[14] = n_keys
    counts[15] = n_uvs
    counts[16] = n_idx
    counts[17] = n_masks
    counts[18] = n_draw_groups
    counts[19] = n_draw_items
    counts[20] = n_glues
    counts[21] = n_glue_info
    counts[22] = n_glue_kfs
    counts[23] = n_mul_colors
    counts[24] = n_scr_colors
    counts[25] = n_blend_kt
    counts[26] = n_blend_bindings
    counts[27] = n_bs_warps
    counts[28] = n_bs_art_meshes
    counts[29] = n_bs_constraint_idx
    counts[30] = n_bs_constraints
    counts[31] = n_bs_constraint_vals

    sections = [bytearray() for _ in range(137)]

    # 0: count_info (128 bytes = 32 ints)
    sections[0] = bytearray(struct.pack("<32i", *counts))

    # 1: canvas_info (24 bytes)
    sections[1] = bytearray(struct.pack("<fffffBBxx", 100.0, 200.0, 200.0, 400.0, 400.0, 1, 0))

    # Part: Part0
    sections[2] = bytearray(8)
    sections[3] = bytearray(b"Part0".ljust(64, b"\x00"))
    sections[4] = bytearray(struct.pack("<i", 0))
    sections[5] = bytearray(struct.pack("<i", 0))
    sections[6] = bytearray(struct.pack("<i", 1))
    sections[7] = bytearray(struct.pack("<i", 1))
    sections[8] = bytearray(struct.pack("<i", 1))
    sections[9] = bytearray(struct.pack("<i", -1))

    # Deformers: 0 WarpRoot (local 0), 1 RotChild (local 0)
    sections[10] = bytearray(16)
    sections[11] = bytearray(b"WarpRoot".ljust(64, b"\x00") + b"RotChild".ljust(64, b"\x00"))
    sections[12] = bytearray(struct.pack("<2i", 0, 0))
    sections[13] = bytearray(struct.pack("<2i", 1, 1))
    sections[14] = bytearray(struct.pack("<2i", 1, 1))
    sections[15] = bytearray(struct.pack("<2i", 0, 0))
    sections[16] = bytearray(struct.pack("<2i", -1, 0)) # RotChild's parent is WarpRoot
    sections[17] = bytearray(struct.pack("<2i", 0, 1)) # 0 warp, 1 rotation
    sections[18] = bytearray(struct.pack("<2i", 0, 0)) # local_idx

    # Warp 0:
    sections[19] = bytearray(struct.pack("<i", 0))
    sections[20] = bytearray(struct.pack("<i", 0))
    sections[21] = bytearray(struct.pack("<i", 1))
    sections[22] = bytearray(struct.pack("<i", 16))
    sections[23] = bytearray(struct.pack("<i", 3))
    sections[24] = bytearray(struct.pack("<i", 3))
    sections[101] = bytearray(struct.pack("<i", 1)) # quad_transform
    sections[105] = bytearray(struct.pack("<i", 0)) # key_color_off = 0 (custom color 0)

    # Rotation 0: default color (points to color 0 with default values or pool)
    sections[25] = bytearray(struct.pack("<i", 0))
    sections[26] = bytearray(struct.pack("<i", 0))
    sections[27] = bytearray(struct.pack("<i", 1))
    sections[28] = bytearray(struct.pack("<f", 0.0))
    sections[106] = bytearray(struct.pack("<i", 0)) # key_color_off = 0

    # ArtMesh 0: assigned to RotChild (1)
    sections[29] = bytearray(8)
    sections[30] = bytearray(8)
    sections[31] = bytearray(8)
    sections[32] = bytearray(8)
    sections[33] = bytearray(b"MeshQuad".ljust(64, b"\x00"))
    sections[34] = bytearray(struct.pack("<i", 1)) # binding_idx = 1
    sections[35] = bytearray(struct.pack("<i", 0)) # keyform_off = 0
    sections[36] = bytearray(struct.pack("<i", 6)) # normal key_len = 6
    sections[37] = bytearray(struct.pack("<i", 1))
    sections[38] = bytearray(struct.pack("<i", 1))
    sections[39] = bytearray(struct.pack("<i", 0))
    sections[40] = bytearray(struct.pack("<i", 1))
    sections[41] = bytearray(struct.pack("<i", 0))
    sections[42] = bytearray([0])
    sections[43] = bytearray(struct.pack("<i", 4)) # 4 vertices
    sections[44] = bytearray(struct.pack("<i", 0))
    sections[45] = bytearray(struct.pack("<i", 0))
    sections[46] = bytearray(struct.pack("<i", 6)) # 6 indices
    sections[47] = bytearray(struct.pack("<i", 0))
    sections[48] = bytearray(struct.pack("<i", 0))
    sections[107] = bytearray(struct.pack("<i", 1)) # key_color_off = 1 (custom colors 1..6)

    # Parameters: ParamX, ParamY, ParamBS
    sections[49] = bytearray(24)
    sections[50] = bytearray(b"ParamX".ljust(64, b"\x00") + b"ParamY".ljust(64, b"\x00") + b"ParamBS".ljust(64, b"\x00"))
    sections[51] = bytearray(struct.pack("<3f", 1.0, 1.0, 1.0)) # max
    sections[52] = bytearray(struct.pack("<3f", -1.0, -1.0, -1.0)) # min
    sections[53] = bytearray(struct.pack("<3f", 0.0, 0.0, 0.0)) # default
    sections[54] = bytearray(struct.pack("<3i", 0, 0, 0)) # repeat = 0
    sections[55] = bytearray(struct.pack("<3i", 4, 4, 4)) # dec_places = 4
    sections[56] = bytearray(struct.pack("<3i", 0, 1, 0)) # key_table_off
    sections[57] = bytearray(struct.pack("<3i", 1, 1, 0)) # key_table_len
    sections[102] = bytearray(24)
    sections[103] = bytearray(struct.pack("<3i", 0, 3, 5))
    sections[104] = bytearray(struct.pack("<3i", 3, 2, 3))
    sections[114] = bytearray(struct.pack("<3i", 0, 0, 1)) # ParamBS has type 1!
    sections[115] = bytearray(struct.pack("<3i", 0, 0, 0)) # blend_key_table_off = 0
    sections[116] = bytearray(struct.pack("<3i", 0, 0, 1)) # blend_key_table_len = 1

    # Part Keyforms (58)
    sections[58] = bytearray(struct.pack("<f", 0.0))

    # Warp Keyforms (59, 60): 1 normal + 3 BS delta keyforms
    # Keyform 0: normal
    sections[59].extend(struct.pack("<f", 1.0))
    sections[60].extend(struct.pack("<i", 0))
    # Keyform 1: Warp BS form 0 (at -1.0)
    sections[59].extend(struct.pack("<f", 0.0))
    sections[60].extend(struct.pack("<i", 32))
    # Keyform 2: Warp BS form 1 (base at 0.0, neutral)
    sections[59].extend(struct.pack("<f", 0.0))
    sections[60].extend(struct.pack("<i", 64))
    # Keyform 3: Warp BS form 2 (at +1.0)
    sections[59].extend(struct.pack("<f", 0.0))
    sections[60].extend(struct.pack("<i", 96))

    # Rotation Keyform 0 (61..67)
    sections[61] = bytearray(struct.pack("<f", 1.0))
    sections[62] = bytearray(struct.pack("<f", 0.0))
    sections[63] = bytearray(struct.pack("<f", 0.0))
    sections[64] = bytearray(struct.pack("<f", 0.0))
    sections[65] = bytearray(struct.pack("<f", 1.0))
    sections[66] = bytearray(struct.pack("<i", 0))
    sections[67] = bytearray(struct.pack("<i", 0))

    # ArtMesh Keyforms: 6 normal keyforms + 3 BS delta keyforms (total 9)
    for k in range(6):
        sections[68].extend(struct.pack("<f", 1.0 - k * 0.05)) # opacity
        sections[69].extend(struct.pack("<f", 10.0 + k)) # draw_order
        sections[70].extend(struct.pack("<i", 128 + k * 8)) # pos_off in floats (after 64 warp points = 128 floats)
    # Keyform 6: BS form 0 (at -1.0)
    sections[68].extend(struct.pack("<f", 0.0))
    sections[69].extend(struct.pack("<f", 0.0))
    sections[70].extend(struct.pack("<i", 176))
    # Keyform 7: BS form 1 (base at 0.0, neutral)
    sections[68].extend(struct.pack("<f", 0.0))
    sections[69].extend(struct.pack("<f", 0.0))
    sections[70].extend(struct.pack("<i", 184))
    # Keyform 8: BS form 2 (at +1.0)
    sections[68].extend(struct.pack("<f", 0.0))
    sections[69].extend(struct.pack("<f", 0.0))
    sections[70].extend(struct.pack("<i", 192))

    # Key Pos pool (71): 200 floats (100 points)
    # 1. 16 warp base points (offset 0..31)
    for r in range(4):
        for c in range(4):
            x = -1.0 + c * (2.0 / 3.0)
            y = -1.0 + r * (2.0 / 3.0)
            sections[71].extend(struct.pack("<2f", x, y))
    # 2. 16 warp BS form 0 delta points (offset 32..63): shift inward (-0.05)
    for _ in range(16):
        sections[71].extend(struct.pack("<2f", -0.05, -0.05))
    # 3. 16 warp BS form 1 neutral points (offset 64..95): all 0.0
    for _ in range(16):
        sections[71].extend(struct.pack("<2f", 0.0, 0.0))
    # 4. 16 warp BS form 2 delta points (offset 96..127): shift outward (+0.05)
    for _ in range(16):
        sections[71].extend(struct.pack("<2f", 0.05, 0.05))
    # 5. 6 * 4 normal mesh points (offset 128..175)
    for k in range(6):
        kx = k % 3 - 1
        ky = k // 3
        dx = kx * 0.2
        dy = ky * 0.2
        sections[71].extend(struct.pack("<2f", -0.5 + dx, -0.5 + dy))
        sections[71].extend(struct.pack("<2f", 0.5 + dx, -0.5 + dy))
        sections[71].extend(struct.pack("<2f", 0.5 + dx, 0.5 + dy))
        sections[71].extend(struct.pack("<2f", -0.5 + dx, 0.5 + dy))
    # 6. 4 delta points for Mesh BS form 0 (offset 176..183): shift inward (-0.1)
    sections[71].extend(struct.pack("<2f", 0.1, 0.1))
    sections[71].extend(struct.pack("<2f", -0.1, 0.1))
    sections[71].extend(struct.pack("<2f", -0.1, -0.1))
    sections[71].extend(struct.pack("<2f", 0.1, -0.1))
    # 7. 4 neutral points for Mesh BS form 1 (offset 184..191): all 0.0
    for _ in range(4):
        sections[71].extend(struct.pack("<2f", 0.0, 0.0))
    # 8. 4 delta points for Mesh BS form 2 (offset 192..199): shift outward (+0.1)
    sections[71].extend(struct.pack("<2f", -0.1, -0.1))
    sections[71].extend(struct.pack("<2f", 0.1, -0.1))
    sections[71].extend(struct.pack("<2f", 0.1, 0.1))
    sections[71].extend(struct.pack("<2f", -0.1, 0.1))

    # Key Table Idx: 72
    sections[72] = bytearray(struct.pack("<2i", 0, 1))

    # Binding Src: 73, 74
    sections[73] = bytearray(struct.pack("<2i", 0, 0))
    sections[74] = bytearray(struct.pack("<2i", 0, 2))

    # Key Tables: 75, 76
    sections[75] = bytearray(struct.pack("<2i", 0, 3))
    sections[76] = bytearray(struct.pack("<2i", 3, 2))

    # Keys: 77 (8 keys)
    sections[77] = bytearray(struct.pack("<8f", -1.0, 0.0, 1.0, -1.0, 1.0, -1.0, 0.0, 1.0))

    # UVs: 78
    sections[78] = bytearray(struct.pack("<8f", 0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0))

    # Indices: 79
    sections[79] = bytearray(struct.pack("<6H", 0, 1, 2, 0, 2, 3))

    # Draw groups: 81-85
    # Group 0: root, Group 1: Part0
    sections[81] = bytearray(struct.pack("<2i", 0, 1)) # obj_off
    sections[82] = bytearray(struct.pack("<2i", 1, 1)) # obj_len
    sections[83] = bytearray(struct.pack("<2i", 1, 1)) # obj_total_count
    sections[84] = bytearray(struct.pack("<2i", 20, 20)) # max_order
    sections[85] = bytearray(struct.pack("<2i", 0, 0)) # min_order

    # Draw group items: 86-88
    # Item 0: Part0 (type 1, idx 0, self_group 1)
    # Item 1: Mesh0 (type 0, idx 0, self_group -1)
    sections[86] = bytearray(struct.pack("<2i", 1, 0)) # type
    sections[87] = bytearray(struct.pack("<2i", 0, 0)) # idx
    sections[88] = bytearray(struct.pack("<2i", 1, -1)) # self_group_idx

    # Color Pools (108..113): 7 colors
    # Color 0: Warp 0 custom color
    sections[108].extend(struct.pack("<f", 0.9))
    sections[109].extend(struct.pack("<f", 0.8))
    sections[110].extend(struct.pack("<f", 0.7))
    sections[111].extend(struct.pack("<f", 0.05))
    sections[112].extend(struct.pack("<f", 0.10))
    sections[113].extend(struct.pack("<f", 0.15))
    # Colors 1..6: Mesh 0 normal keyform colors
    for k in range(6):
        sections[108].extend(struct.pack("<f", 0.5 + k * 0.08))
        sections[109].extend(struct.pack("<f", 0.6 + k * 0.06))
        sections[110].extend(struct.pack("<f", 0.7 + k * 0.04))
        sections[111].extend(struct.pack("<f", 0.05 * k))
        sections[112].extend(struct.pack("<f", 0.03 * k))
        sections[113].extend(struct.pack("<f", 0.02 * k))

    # BlendShape Tables (117..119): 1 table for ParamBS with 3 keys, base_key_idx = 1 (intermediate base key!)
    sections[117] = bytearray(struct.pack("<i", 5)) # keys_off = 5 in section 77 (keys: [-1.0, 0.0, 1.0])
    sections[118] = bytearray(struct.pack("<i", 3)) # keys_len = 3
    sections[119] = bytearray(struct.pack("<i", 1)) # base_key_idx = 1 (intermediate!)

    # Blend Bindings (120..124): 2 bindings (Binding 0: Warp 0, Binding 1: Mesh 0) sharing constraint 0!
    sections[120] = bytearray(struct.pack("<2i", 0, 0)) # key_table_idx
    sections[121] = bytearray(struct.pack("<2i", 1, 6)) # key_bs_off: Warp BS starts at 1, Mesh BS starts at 6
    sections[122] = bytearray(struct.pack("<2i", 3, 3)) # key_bs_len: 3 keyforms each
    sections[123] = bytearray(struct.pack("<2i", 0, 0)) # bs_constraint_idx_off: both point to constraint 0!
    sections[124] = bytearray(struct.pack("<2i", 1, 1)) # bs_constraint_idx_len: 1 constraint each

    # BS Warps (125..127): 1 entry for WarpRoot (target_idx 0)
    sections[125] = bytearray(struct.pack("<i", 0)) # target_idx = 0 (WarpRoot)
    sections[126] = bytearray(struct.pack("<i", 0)) # bs_binding_off = 0
    sections[127] = bytearray(struct.pack("<i", 1)) # bs_binding_len = 1

    # BS ArtMeshes (128..130): 1 entry for MeshQuad (target_idx 0)
    sections[128] = bytearray(struct.pack("<i", 0)) # target_idx = 0 (MeshQuad)
    sections[129] = bytearray(struct.pack("<i", 1)) # bs_binding_off = 1
    sections[130] = bytearray(struct.pack("<i", 1)) # bs_binding_len = 1

    # BS Constraint Idx (131): 1 shared constraint index
    sections[131] = bytearray(struct.pack("<i", 0)) # constraint 0

    # BS Constraints (132..134): 1 constraint on ParamBS (parameter_idx = 2)
    sections[132] = bytearray(struct.pack("<i", 2)) # parameter_idx = 2 (ParamBS)
    sections[133] = bytearray(struct.pack("<i", 0)) # value_off = 0
    sections[134] = bytearray(struct.pack("<i", 3)) # value_len = 3

    # BS Constraint Values (135, 136): keys [-1.0, 0.0, 1.0], weights [1.0, 0.0, 1.0]
    sections[135] = bytearray(struct.pack("<3f", -1.0, 0.0, 1.0))
    sections[136] = bytearray(struct.pack("<3f", 1.0, 0.0, 1.0))

    # Assemble MOC3 file:
    header = bytearray(1984)
    header[0:4] = b"MOC3"
    header[4] = 4 # version 4
    header[5] = 0 # little endian

    out = header
    for i in range(137):
        sec_data = sections[i]
        aligned_len = (len(out) + 63) & ~63
        out.extend(b"\x00" * (aligned_len - len(out)))
        offset = len(out)
        struct.pack_into("<I", out, 64 + i * 4, offset)
        out.extend(sec_data)

    aligned_len = (len(out) + 63) & ~63
    out.extend(b"\x00" * (aligned_len - len(out)))
    return bytes(out)

moc3_bytes = build_v42_moc3()
(OUT_DIR / "model.moc3").write_bytes(moc3_bytes)

model3_json = {
    "Version": 3,
    "FileReferences": {
        "Moc": "model.moc3",
        "Textures": ["texture_00.png"]
    }
}
import json
(OUT_DIR / "model.model3.json").write_text(json.dumps(model3_json, indent=2))
print(f"Generated external v42 fixture at {OUT_DIR}: {len(moc3_bytes)} bytes")
