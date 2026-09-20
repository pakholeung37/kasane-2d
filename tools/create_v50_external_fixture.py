#!/usr/bin/env python3
"""Generate an independent external v50 (csmMocVersion_50) MOC3 fixture for M3 validation.
Constructed directly via raw binary struct layout (independent of kasane_moc3::encode_moc3).
"""
import os
import struct
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT_DIR = ROOT / "tests/fixtures/external_v50"
OUT_DIR.mkdir(parents=True, exist_ok=True)

# Generate 1x1 white PNG texture
import zlib
def create_dummy_png(path: Path):
    w, h = 4, 4
    raw_data = bytearray()
    for y in range(h):
        raw_data.append(0) # filter byte
        raw_data.extend([255, 255, 255, 255] * w)
    compressed = zlib.compress(bytes(raw_data))
    
    png = bytearray(b"\x89PNG\r\n\x1a\n")
    # IHDR
    ihdr_data = struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0)
    ihdr_crc = zlib.crc32(b"IHDR" + ihdr_data)
    png.extend(struct.pack(">I4s", len(ihdr_data), b"IHDR") + ihdr_data + struct.pack(">I", ihdr_crc))
    # IDAT
    idat_crc = zlib.crc32(b"IDAT" + compressed)
    png.extend(struct.pack(">I4s", len(compressed), b"IDAT") + compressed + struct.pack(">I", idat_crc))
    # IEND
    iend_crc = zlib.crc32(b"IEND")
    png.extend(struct.pack(">I4s", 0, b"IEND") + struct.pack(">I", iend_crc))
    path.write_bytes(png)

create_dummy_png(OUT_DIR / "texture_00.png")

# Construct MOC3 v5 binary
def build_v50_moc3():
    # Schema section names and widths (152 sections)
    # We follow Purism moc3.h schema
    # Counts:
    n_parts = 1
    n_deformers = 2 # 1 warp, 1 rotation
    n_warps = 1
    n_rotations = 1
    n_meshes = 1
    n_params = 2
    n_part_kfs = 1
    n_warp_kfs = 1
    n_rot_kfs = 1
    n_mesh_kfs = 6 # 3 x 2 grid
    n_kf_pos = (16 * 2 * n_warp_kfs) + (4 * 2 * n_mesh_kfs) # warp 3x3 grid (16 pts) + mesh 4 pts
    n_kt_idx = 2
    n_bindings = 2 # 0: static, 1: mesh 2-axis
    n_kt = 2
    n_keys = 5 # ParamX: 3 keys [-1, 0, 1], ParamY: 2 keys [-1, 1]
    n_uvs = 8 # 4 vertices * 2
    n_idx = 6 # 2 triangles * 3
    n_masks = 0
    n_draw_groups = 2 # root + part
    n_draw_items = 2
    n_mul_colors = 6
    n_scr_colors = 6
    
    counts = [0] * 64
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
    counts[23] = n_mul_colors
    counts[24] = n_scr_colors

    sections = [bytearray() for _ in range(152)]

    # 0: count_info (256 bytes)
    sections[0] = bytearray(struct.pack("<64i", *counts))

    # 1: canvas_info (24 bytes)
    # ppu, ox, oy, w, h, flag
    sections[1] = bytearray(struct.pack("<fffffBBxx", 100.0, 200.0, 200.0, 400.0, 400.0, 1, 0))

    # Part: Part0
    # 2: id_runtime (8 bytes null)
    sections[2] = bytearray(8)
    # 3: id (64 bytes)
    sections[3] = bytearray(b"Part0".ljust(64, b"\x00"))
    # 4: binding_idx
    sections[4] = bytearray(struct.pack("<i", 0))
    # 5: keyform_off
    sections[5] = bytearray(struct.pack("<i", 0))
    # 6: key_len
    sections[6] = bytearray(struct.pack("<i", 1))
    # 7: visible
    sections[7] = bytearray(struct.pack("<i", 1))
    # 8: enable
    sections[8] = bytearray(struct.pack("<i", 1))
    # 9: parent_part_idx
    sections[9] = bytearray(struct.pack("<i", -1))

    # Deformers:
    # 0: WarpRoot (type 0, local 0, parent part 0, parent def -1)
    # 1: RotChild (type 1, local 0, parent part 0, parent def 0)
    sections[10] = bytearray(16) # id_runtime
    sections[11] = bytearray(b"WarpRoot".ljust(64, b"\x00") + b"RotChild".ljust(64, b"\x00"))
    sections[12] = bytearray(struct.pack("<2i", 0, 0)) # binding_idx
    sections[13] = bytearray(struct.pack("<2i", 1, 1)) # visible
    sections[14] = bytearray(struct.pack("<2i", 1, 1)) # enable
    sections[15] = bytearray(struct.pack("<2i", 0, 0)) # parent_part_idx
    sections[16] = bytearray(struct.pack("<2i", -1, 0)) # parent_deformer_idx: RotChild has parent WarpRoot!
    sections[17] = bytearray(struct.pack("<2i", 0, 1)) # type: 0 warp, 1 rotation
    sections[18] = bytearray(struct.pack("<2i", 0, 0)) # local_idx

    # Warp 0:
    sections[19] = bytearray(struct.pack("<i", 0)) # binding_idx
    sections[20] = bytearray(struct.pack("<i", 0)) # keyform_off
    sections[21] = bytearray(struct.pack("<i", 1)) # key_len
    sections[22] = bytearray(struct.pack("<i", 16)) # vertex_count: (3+1)*(3+1) = 16
    sections[23] = bytearray(struct.pack("<i", 3)) # row
    sections[24] = bytearray(struct.pack("<i", 3)) # col
    sections[101] = bytearray(struct.pack("<i", 1)) # quad_transform
    sections[105] = bytearray(struct.pack("<i", 0)) # key_color_off

    # Rotation 0:
    sections[25] = bytearray(struct.pack("<i", 0)) # binding_idx
    sections[26] = bytearray(struct.pack("<i", 0)) # keyform_off
    sections[27] = bytearray(struct.pack("<i", 1)) # key_len
    sections[28] = bytearray(struct.pack("<f", 0.0)) # base_angle
    sections[106] = bytearray(struct.pack("<i", 0)) # key_color_off

    # ArtMesh 0: assigned to RotChild (parent def 1)
    sections[29] = bytearray(8) # id_runtime
    sections[30] = bytearray(8) # uv_runtime
    sections[31] = bytearray(8) # pos_idx_runtime
    sections[32] = bytearray(8) # mask_runtime
    sections[33] = bytearray(b"MeshQuad".ljust(64, b"\x00"))
    sections[34] = bytearray(struct.pack("<i", 1)) # binding_idx = 1
    sections[35] = bytearray(struct.pack("<i", 0)) # keyform_off = 0
    sections[36] = bytearray(struct.pack("<i", 6)) # key_len = 6
    sections[37] = bytearray(struct.pack("<i", 1)) # visible
    sections[38] = bytearray(struct.pack("<i", 1)) # enable
    sections[39] = bytearray(struct.pack("<i", 0)) # parent_part_idx
    sections[40] = bytearray(struct.pack("<i", 1)) # parent_deformer_idx = RotChild (1)
    sections[41] = bytearray(struct.pack("<i", 0)) # texture_no = 0
    sections[42] = bytearray([0]) # normal blend, single-sided, no inv mask
    sections[43] = bytearray(struct.pack("<i", 4)) # vertex_count = 4
    sections[44] = bytearray(struct.pack("<i", 0)) # uv_off
    sections[45] = bytearray(struct.pack("<i", 0)) # idx_off
    sections[46] = bytearray(struct.pack("<i", 6)) # idx_len = 6
    sections[47] = bytearray(struct.pack("<i", 0)) # mask_off
    sections[48] = bytearray(struct.pack("<i", 0)) # mask_len
    sections[107] = bytearray(struct.pack("<i", 0)) # key_color_off

    # Parameters: ParamX (3 keys), ParamY (2 keys)
    sections[49] = bytearray(16) # id_runtime
    sections[50] = bytearray(b"ParamX".ljust(64, b"\x00") + b"ParamY".ljust(64, b"\x00"))
    sections[51] = bytearray(struct.pack("<2f", 1.0, 1.0)) # max
    sections[52] = bytearray(struct.pack("<2f", -1.0, -1.0)) # min
    sections[53] = bytearray(struct.pack("<2f", 0.0, 0.0)) # default
    sections[54] = bytearray(struct.pack("<2i", 0, 0)) # repeat = 0
    sections[55] = bytearray(struct.pack("<2i", 4, 4)) # decimal_places = 4
    sections[56] = bytearray(struct.pack("<2i", 0, 1)) # key_table_off
    sections[57] = bytearray(struct.pack("<2i", 1, 1)) # key_table_len
    sections[102] = bytearray(16) # key_runtime
    sections[103] = bytearray(struct.pack("<2i", 0, 3)) # param_keys_src.keys_off
    sections[104] = bytearray(struct.pack("<2i", 3, 2)) # param_keys_src.keys_len
    sections[114] = bytearray(struct.pack("<2i", 0, 0)) # type
    sections[115] = bytearray(struct.pack("<2i", 0, 0)) # blend_key_table_off
    sections[116] = bytearray(struct.pack("<2i", 0, 0)) # blend_key_table_len

    # Part Keyforms: 58
    sections[58] = bytearray(struct.pack("<f", 0.0))

    # Warp Keyform 0:
    sections[59] = bytearray(struct.pack("<f", 1.0)) # opacity
    sections[60] = bytearray(struct.pack("<i", 0)) # key_pos_off = 0
    sections[137] = bytearray(struct.pack("<i", 0)) # key_mul_color_off
    sections[138] = bytearray(struct.pack("<i", 0)) # key_scr_color_off

    # Rotation Keyform 0:
    sections[61] = bytearray(struct.pack("<f", 1.0)) # opacity
    sections[62] = bytearray(struct.pack("<f", 0.0)) # angle
    sections[63] = bytearray(struct.pack("<f", 0.0)) # ox
    sections[64] = bytearray(struct.pack("<f", 0.0)) # oy
    sections[65] = bytearray(struct.pack("<f", 1.0)) # scale
    sections[66] = bytearray(struct.pack("<i", 0)) # reflect_x
    sections[67] = bytearray(struct.pack("<i", 0)) # reflect_y
    sections[139] = bytearray(struct.pack("<i", 0)) # key_mul_color_off
    sections[140] = bytearray(struct.pack("<i", 0)) # key_scr_color_off

    # ArtMesh Keyforms (6 keyforms for 3x2 grid):
    # Distinct opacities, draw orders, and positions
    # Keyform k in 0..6:
    for k in range(6):
        sections[68].extend(struct.pack("<f", 1.0 - k * 0.05)) # opacity
        sections[69].extend(struct.pack("<f", 10.0 + k)) # draw_order
        sections[70].extend(struct.pack("<i", 32 + k * 8)) # key_pos_off (after 16 warp points = 32 floats)
        sections[141].extend(struct.pack("<i", k)) # key_mul_color_off
        sections[142].extend(struct.pack("<i", k)) # key_scr_color_off

    # Key Pos pool (71):
    # 16 points for Warp 0: 4x4 grid from (-1, -1) to (1, 1)
    for r in range(4):
        for c in range(4):
            x = -1.0 + c * (2.0 / 3.0)
            y = -1.0 + r * (2.0 / 3.0)
            sections[71].extend(struct.pack("<2f", x, y))
    # 6 * 4 points for Mesh 0: quad in local coordinates of RotChild
    for k in range(6):
        kx = k % 3 - 1 # -1, 0, 1
        ky = k // 3 # 0, 1
        dx = kx * 0.2
        dy = ky * 0.2
        # quad: (-0.5, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)
        sections[71].extend(struct.pack("<2f", -0.5 + dx, -0.5 + dy))
        sections[71].extend(struct.pack("<2f", 0.5 + dx, -0.5 + dy))
        sections[71].extend(struct.pack("<2f", 0.5 + dx, 0.5 + dy))
        sections[71].extend(struct.pack("<2f", -0.5 + dx, 0.5 + dy))

    # Key Table Idx: 72
    sections[72] = bytearray(struct.pack("<2i", 0, 1))

    # Binding Src: 73, 74
    # Binding 0: static
    # Binding 1: mesh binding (2 axes)
    sections[73] = bytearray(struct.pack("<2i", 0, 0)) # key_table_idx_off
    sections[74] = bytearray(struct.pack("<2i", 0, 2)) # key_table_idx_len: 0 and 2 axes

    # Key Tables: 75, 76
    sections[75] = bytearray(struct.pack("<2i", 0, 3)) # keys_off
    sections[76] = bytearray(struct.pack("<2i", 3, 2)) # keys_len: 3 and 2

    # Keys: 77
    sections[77] = bytearray(struct.pack("<5f", -1.0, 0.0, 1.0, -1.0, 1.0))

    # UVs: 78
    sections[78] = bytearray(struct.pack("<8f", 0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0))

    # Indices: 79 (2 triangles)
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

    # Color tables: 108-113 (6 distinct multiply and screen colors)
    for k in range(6):
        sections[108].extend(struct.pack("<f", 0.5 + k * 0.08)) # mul_r
        sections[109].extend(struct.pack("<f", 0.6 + k * 0.06)) # mul_g
        sections[110].extend(struct.pack("<f", 0.7 + k * 0.04)) # mul_b
        sections[111].extend(struct.pack("<f", 0.05 * k)) # scr_r
        sections[112].extend(struct.pack("<f", 0.03 * k)) # scr_g
        sections[113].extend(struct.pack("<f", 0.02 * k)) # scr_b

    # Assemble MOC3 file:
    # Header: 64 bytes
    # Offsets: 160 * 4 = 640 bytes (total 704 bytes)
    # Padded to 1984 bytes before section 0
    header = bytearray(1984)
    header[0:4] = b"MOC3"
    header[4] = 5 # version 5
    header[5] = 0 # little endian

    out = header
    for i in range(152):
        sec_data = sections[i]
        # Align to 64 bytes
        aligned_len = (len(out) + 63) & ~63
        out.extend(b"\x00" * (aligned_len - len(out)))
        offset = len(out)
        struct.pack_into("<I", out, 64 + i * 4, offset)
        out.extend(sec_data)

    aligned_len = (len(out) + 63) & ~63
    out.extend(b"\x00" * (aligned_len - len(out)))
    return bytes(out)

moc3_bytes = build_v50_moc3()
(OUT_DIR / "model.moc3").write_bytes(moc3_bytes)

# Write model3.json
model3_json = {
    "Version": 3,
    "FileReferences": {
        "Moc": "model.moc3",
        "Textures": ["texture_00.png"],
        "Physics": "model.physics3.json",
        "Motions": {"Idle": [{"File": "motions/idle.motion3.json"}]}
    }
}
import json
(OUT_DIR / "model.model3.json").write_text(json.dumps(model3_json, indent=2))

print(f"Generated external v50 fixture at {OUT_DIR}: {len(moc3_bytes)} bytes")
