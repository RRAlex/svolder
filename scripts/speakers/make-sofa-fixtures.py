#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later

"""Write the synthetic SOFA files the svolder-speakers tests read (`make speakers-fixtures`, into crates/svolder-speakers/tests/fixtures, where the files are
committed so that nothing else needs h5py).

Usage: make-sofa-fixtures.py OUTDIR

Every response is a Dirac at a known sample and gain, so the converter's output can be checked
to the sample. Nothing here is measured data. Needs h5py (apt: python3-h5py).
"""

import os
import sys

try:
    import h5py
    import numpy as np
except ImportError as error:
    sys.exit(f"make-sofa-fixtures: {error}; install it with: sudo apt install python3-h5py")

N = 256
# The four paths of the speaker pair: (sample, gain) for left->left ear, left->right ear,
# right->left ear, right->right ear.
PATHS = [(10, 0.5), (14, 0.25), (20, 0.125), (30, 0.75)]


def open_file(path):
    """A new HDF5 file as netCDF-4 writes SOFA files: version 2 object headers, creation order
    tracked."""
    return h5py.File(path, "w", libver=("v108", "v108"), track_order=True)


def text(value):
    return np.bytes_(value)


def write(path, *, convention="SimpleFreeFieldHRIR", data_type="FIR", rate=48000.0,
          positions=None, kind="spherical", delay=None, swap_receivers=False, emitters=None,
          ir_override=None):
    """One SOFA file: `positions` are (azimuth, elevation, distance) per measurement."""
    if positions is None:
        # 0: front, 1: left speaker, 2: right speaker, 3: a raised measurement at the left.
        positions = [(0.0, 0.0, 1.5), (30.0, 0.0, 1.5), (330.0, 0.0, 1.5), (30.0, 17.5, 1.5)]
    m = len(positions)
    ir = np.zeros((m, 2, N))
    for k, (az, el, _) in enumerate(positions):
        # The distractors get a loud tap well away from the paths.
        ir[k, :, 100] = 9.0
        if el == 0.0 and abs(((az + 180) % 360) - 180) in (30.0, 45.0):
            left_speaker = az < 180
            first = 0 if left_speaker else 2
            for ear in range(2):
                at, gain = PATHS[first + ear]
                ir[k, ear, :] = 0.0
                ir[k, ear, at] = gain
    if ir_override is not None:
        ir = ir_override
    if kind == "cartesian":
        src = np.array([[np.cos(np.radians(el)) * np.cos(np.radians(az)) * r,
                         np.cos(np.radians(el)) * np.sin(np.radians(az)) * r,
                         np.sin(np.radians(el)) * r] for az, el, r in positions])
    else:
        src = np.array(positions, dtype=float)
    with open_file(path) as f:
        f.attrs["Conventions"] = text("SOFA")
        f.attrs["SOFAConventions"] = text(convention)
        f.attrs["DataType"] = text(data_type)
        f.attrs["Version"] = text("1.0")
        ds = f.create_dataset("Data.IR", data=ir)
        f.create_dataset("Data.SamplingRate", data=np.array([rate]))
        f.create_dataset("Data.Delay", data=np.array([delay or [0.0, 0.0]]))
        s = f.create_dataset("SourcePosition", data=src)
        s.attrs["Type"] = text(kind)
        s.attrs["Units"] = text("degree, degree, metre" if kind == "spherical" else "metre")
        side = [-0.09, 0.09] if swap_receivers else [0.09, -0.09]
        r = f.create_dataset("ReceiverPosition", data=np.array([[[0.0], [side[0]], [0.0]],
                                                                  [[0.0], [side[1]], [0.0]]]))
        r.attrs["Type"] = text("cartesian")
        f.create_dataset("ListenerPosition", data=np.zeros((1, 3)))
        f.create_dataset("ListenerView", data=np.array([[1.0, 0.0, 0.0]]))
        f.create_dataset("ListenerUp", data=np.array([[0.0, 0.0, 1.0]]))
        f.create_dataset("EmitterPosition", data=np.zeros((1, 3, 1)))
        del ds


def main():
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    out = sys.argv[1]
    os.makedirs(out, exist_ok=True)
    p = lambda name: os.path.join(out, name)
    write(p("free-field.sofa"))
    write(p("free-field-44100.sofa"), rate=44100.0)
    write(p("cartesian.sofa"), kind="cartesian")
    write(p("delay.sofa"), delay=[3.0, 7.0])
    write(p("swapped-receivers.sofa"), swap_receivers=True)
    write(p("not-fir.sofa"), data_type="TF")
    write(p("room.sofa"), convention="SingleRoomSRIR",
          positions=[(0.0, 0.0, 1.5), (45.0, 0.0, 1.5), (315.0, 0.0, 1.5), (90.0, 0.0, 1.5)])
    mimo = np.zeros((1, 2, 2, N))
    mimo[0, :, :, 5] = 1.0
    write(p("mimo.sofa"), convention="MultiSpeakerBRIR", positions=[(0.0, 0.0, 1.5)],
          ir_override=mimo)
    print(f"wrote 8 files to {out}")


main()
