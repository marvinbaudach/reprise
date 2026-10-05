/* Feeds f32 mono PCM through the original cavacore, one 735-sample hop per
 * call (the desktop's 60 Hz cadence at 44.1 kHz), and writes one CSV row of
 * 64 bars per hop. Also writes the 65 cutoff frequencies.
 *
 * usage: oracle in.f32 out.csv cutoffs.txt
 *
 * cava_init(64 bars, 44100 Hz, mono, autosens on, noise reduction 0.77,
 * 50 Hz - 10 kHz). Input is scaled by 65535, which is what the port's
 * CAVA_FIXED_POINT_SCALE does. cavacore estimates its own framerate from the
 * sample counts, exactly as the port does, so there is no fixed framerate.
 */
#include <stdio.h>
#include <stdlib.h>
#include "cavacore.h"

#define HOP 735
#define BARS 64

int main(int argc, char **argv) {
    if (argc != 4) {
        fprintf(stderr, "usage: %s in.f32 out.csv cutoffs.txt\n", argv[0]);
        return 2;
    }
    FILE *in_file = fopen(argv[1], "rb");
    FILE *out_file = fopen(argv[2], "w");
    FILE *cutoff_file = fopen(argv[3], "w");
    if (!in_file || !out_file || !cutoff_file) {
        perror("open");
        return 2;
    }
    struct cava_plan *plan = cava_init(BARS, 44100, 1, 1, 0.77, 50, 10000);
    if (plan->status != 0) {
        fprintf(stderr, "init status %d: %s\n", plan->status, plan->error_message);
        return 1;
    }
    for (int i = 0; i <= BARS; i++) {
        fprintf(cutoff_file, "%.6f\n", (double)plan->cut_off_frequency[i]);
    }
    float chunk[HOP];
    double samples[HOP];
    double bars[BARS];
    while (fread(chunk, sizeof(float), HOP, in_file) == HOP) {
        for (int i = 0; i < HOP; i++) {
            samples[i] = (double)chunk[i] * 65535.0;
        }
        cava_execute(samples, HOP, bars, plan);
        for (int i = 0; i < BARS; i++) {
            fprintf(out_file, i ? ",%.6f" : "%.6f", bars[i]);
        }
        fputc('\n', out_file);
    }
    cava_destroy(plan);
    return 0;
}
