# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Fit the four constants of the computer's loudness stage to the ISO 226:2003 contours.

Run it with `make loudness-fit` (python3 -I, numpy only). It prints the constants pinned in
crates/svolder-host/src/eq/loudness.rs and how far the stage's shelves are from the standard.

The target is the difference curve of the standard,

    B(f, d) = Lp(f, P - d) - (Lp(f, P) - d)          P = 80 phon

the level the contour at P - d phon needs over the one at P, less the d dB the volume itself took
off, so B(1 kHz, d) = 0 and the bass and the treble stand above it. The stage's two shelves,
before the cut-only scaling, are the model:

    low  shelf: zero at FZ_LO, DC gain  K_LO * d dB
    high shelf: zero at FZ_HI, top gain min(K_HI * d, 6) dB

The four constants (FZ_LO, K_LO, FZ_HI, K_HI) minimise the largest error over d = 5..40 dB (step 1)
and the 29 frequencies of the standard, each depth's error divided by its budget: 1.2 dB up to
10 dB of depth, 1.5 dB at 20, rising to 2.6 at 40.
"""
import numpy as np

# f [Hz], alpha_f, L_U [dB], T_f [dB]: ISO 226:2003 table 1.
TABLE = [
    (20, 0.532, -31.6, 78.1), (25, 0.506, -27.2, 68.7), (31.5, 0.480, -23.0, 59.5),
    (40, 0.455, -19.1, 51.1), (50, 0.432, -15.9, 44.0), (63, 0.409, -13.0, 37.5),
    (80, 0.387, -10.3, 31.5), (100, 0.367, -8.1, 26.5), (125, 0.349, -6.2, 22.1),
    (160, 0.330, -4.5, 17.9), (200, 0.315, -3.1, 14.4), (250, 0.301, -2.0, 11.4),
    (315, 0.288, -1.1, 8.6), (400, 0.276, -0.4, 6.2), (500, 0.267, 0.0, 4.4),
    (630, 0.259, 0.3, 3.0), (800, 0.253, 0.5, 2.2), (1000, 0.250, 0.0, 2.4),
    (1250, 0.246, -2.7, 3.5), (1600, 0.244, -4.1, 1.7), (2000, 0.243, -1.0, -1.3),
    (2500, 0.243, 1.7, -4.2), (3150, 0.243, 2.5, -6.0), (4000, 0.242, 1.2, -5.4),
    (5000, 0.242, -2.1, -1.5), (6300, 0.245, -7.1, 6.0), (8000, 0.254, -11.2, 12.6),
    (10000, 0.271, -10.7, 13.9), (12500, 0.301, -3.1, 12.3),
]
F, ALPHA, L_U, T_F = np.array(TABLE).T
REFERENCE_PHON = 80.0
HIGH_CAP_DB = 6.0
DEPTHS = np.arange(5.0, 40.5, 1.0)


def contour(phon):
    """The sound pressure level in dB of the contour at `phon` at each of the 29 frequencies."""
    a = 4.47e-3 * (10 ** (0.025 * phon) - 1.15) + (0.4 * 10 ** ((T_F + L_U) / 10 - 9)) ** ALPHA
    return 10 / ALPHA * np.log10(a) - L_U + 94


def target(d):
    return contour(REFERENCE_PHON - d) - (contour(REFERENCE_PHON) - d)


def model(p, d):
    """The two shelves' summed analog response in dB at the 29 frequencies, depth d."""
    fz_lo, k_lo, fz_hi, k_hi = p
    g_lo, g_hi = k_lo * d, min(k_hi * d, HIGH_CAP_DB)
    fp_lo, fp_hi = fz_lo * 10 ** (-g_lo / 20), fz_hi * 10 ** (g_hi / 20)
    s = 1j * F
    h = abs((s + fz_lo) / (s + fp_lo)) * abs((1 + s / fz_hi) / (1 + s / fp_hi))
    return 20 * np.log10(h)


def budget(d):
    if d <= 10:
        return 1.2
    return 1.2 + (d - 10) * 0.03 if d <= 20 else 1.5 + (d - 20) * 0.055


def cost(p):
    return max(np.max(np.abs(model(p, d) - target(d))) / budget(d) for d in DEPTHS)


def errors(p, depths=(10, 20, 40)):
    return [float(np.max(np.abs(model(p, d) - target(d)))) for d in depths]


def nelder_mead(f, x0, step, iterations):
    n = len(x0)
    start = np.array(x0, float)
    simplex = [start] + [start + np.eye(n)[i] * step[i] for i in range(n)]
    values = [f(x) for x in simplex]
    for _ in range(iterations):
        order = np.argsort(values)
        simplex = [simplex[i] for i in order]
        values = [values[i] for i in order]
        centre = np.mean(simplex[:-1], axis=0)
        reflected = centre + (centre - simplex[-1])
        fr = f(reflected)
        if fr < values[0]:
            expanded = centre + 2 * (centre - simplex[-1])
            fe = f(expanded)
            simplex[-1], values[-1] = (expanded, fe) if fe < fr else (reflected, fr)
        elif fr < values[-2]:
            simplex[-1], values[-1] = reflected, fr
        else:
            contracted = centre + 0.5 * (simplex[-1] - centre)
            fc = f(contracted)
            if fc < values[-1]:
                simplex[-1], values[-1] = contracted, fc
            else:
                best = simplex[0]
                simplex = [best] + [best + 0.5 * (x - best) for x in simplex[1:]]
                values = [values[0]] + [f(x) for x in simplex[1:]]
    best = int(np.argmin(values))
    return simplex[best], values[best]


def main():
    print("ISO 226:2003 at 1 kHz, 40 phon: %.2f dB (the standard says 40)" % contour(40.0)[17])
    best = None
    for start in ([320, 0.5, 10000, 0.28], [290, 0.52, 7800, 0.15], [250, 0.6, 6000, 0.3]):
        x, v = nelder_mead(cost, start, [30, 0.04, 800, 0.03], 700)
        x, v = nelder_mead(cost, x, [8, 0.01, 200, 0.01], 700)
        if best is None or v < best[1]:
            best = (x, v)
    fit = best[0]
    pinned = np.array([round(fit[0]), round(fit[1] * 200) / 200, round(fit[2] / 10) * 10,
                       round(fit[3] * 200) / 200])
    print("fit:    FZ_LO %.1f  K_LO %.4f  FZ_HI %.0f  K_HI %.4f" % tuple(fit))
    print("errors at d = 10, 20, 40 dB: %.2f %.2f %.2f dB" % tuple(errors(fit)))
    print("pinned: LOW_CORNER_HZ %.1f  LOW_GAIN_PER_DB %.3f  HIGH_CORNER_HZ %.1f"
          "  HIGH_GAIN_PER_DB %.3f" % tuple(pinned))
    print("errors of the pinned constants at d = 5, 10, 20, 30, 40 dB: " +
          " ".join("%.2f" % e for e in errors(pinned, (5, 10, 20, 30, 40))))


if __name__ == "__main__":
    main()
