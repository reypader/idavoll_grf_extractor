// GRF DES decryption — translated from ROBrowser's GameFileDecrypt.js.
// This is a custom keyless DES variant used by Gravity for GRF encryption.
// Most GRF entries (type 0x01) are not encrypted; this is only needed for
// ENCRYPT_MIXED (0x02) and ENCRYPT_HEADER (0x04) entries.

// ---------------------------------------------------------------------------
// Permutation / substitution tables
// ---------------------------------------------------------------------------

#[rustfmt::skip]
const IP_TABLE: [u8; 64] = [
    58, 50, 42, 34, 26, 18, 10,  2,
    60, 52, 44, 36, 28, 20, 12,  4,
    62, 54, 46, 38, 30, 22, 14,  6,
    64, 56, 48, 40, 32, 24, 16,  8,
    57, 49, 41, 33, 25, 17,  9,  1,
    59, 51, 43, 35, 27, 19, 11,  3,
    61, 53, 45, 37, 29, 21, 13,  5,
    63, 55, 47, 39, 31, 23, 15,  7,
];

#[rustfmt::skip]
const FP_TABLE: [u8; 64] = [
    40,  8, 48, 16, 56, 24, 64, 32,
    39,  7, 47, 15, 55, 23, 63, 31,
    38,  6, 46, 14, 54, 22, 62, 30,
    37,  5, 45, 13, 53, 21, 61, 29,
    36,  4, 44, 12, 52, 20, 60, 28,
    35,  3, 43, 11, 51, 19, 59, 27,
    34,  2, 42, 10, 50, 18, 58, 26,
    33,  1, 41,  9, 49, 17, 57, 25,
];

#[rustfmt::skip]
const P_TABLE: [u8; 32] = [
    16,  7, 20, 21,
    29, 12, 28, 17,
     1, 15, 23, 26,
     5, 18, 31, 10,
     2,  8, 24, 14,
    32, 27,  3,  9,
    19, 13, 30,  6,
    22, 11,  4, 25,
];

#[rustfmt::skip]
const SBOX: [[u8; 64]; 4] = [
    [
        0xef, 0x03, 0x41, 0xfd, 0xd8, 0x74, 0x1e, 0x47,  0x26, 0xef, 0xfb, 0x22, 0xb3, 0xd8, 0x84, 0x1e,
        0x39, 0xac, 0xa7, 0x60, 0x62, 0xc1, 0xcd, 0xba,  0x5c, 0x96, 0x90, 0x59, 0x05, 0x3b, 0x7a, 0x85,
        0x40, 0xfd, 0x1e, 0xc8, 0xe7, 0x8a, 0x8b, 0x21,  0xda, 0x43, 0x64, 0x9f, 0x2d, 0x14, 0xb1, 0x72,
        0xf5, 0x5b, 0xc8, 0xb6, 0x9c, 0x37, 0x76, 0xec,  0x39, 0xa0, 0xa3, 0x05, 0x52, 0x6e, 0x0f, 0xd9,
    ],
    [
        0xa7, 0xdd, 0x0d, 0x78, 0x9e, 0x0b, 0xe3, 0x95,  0x60, 0x36, 0x36, 0x4f, 0xf9, 0x60, 0x5a, 0xa3,
        0x11, 0x24, 0xd2, 0x87, 0xc8, 0x52, 0x75, 0xec,  0xbb, 0xc1, 0x4c, 0xba, 0x24, 0xfe, 0x8f, 0x19,
        0xda, 0x13, 0x66, 0xaf, 0x49, 0xd0, 0x90, 0x06,  0x8c, 0x6a, 0xfb, 0x91, 0x37, 0x8d, 0x0d, 0x78,
        0xbf, 0x49, 0x11, 0xf4, 0x23, 0xe5, 0xce, 0x3b,  0x55, 0xbc, 0xa2, 0x57, 0xe8, 0x22, 0x74, 0xce,
    ],
    [
        0x2c, 0xea, 0xc1, 0xbf, 0x4a, 0x24, 0x1f, 0xc2,  0x79, 0x47, 0xa2, 0x7c, 0xb6, 0xd9, 0x68, 0x15,
        0x80, 0x56, 0x5d, 0x01, 0x33, 0xfd, 0xf4, 0xae,  0xde, 0x30, 0x07, 0x9b, 0xe5, 0x83, 0x9b, 0x68,
        0x49, 0xb4, 0x2e, 0x83, 0x1f, 0xc2, 0xb5, 0x7c,  0xa2, 0x19, 0xd8, 0xe5, 0x7c, 0x2f, 0x83, 0xda,
        0xf7, 0x6b, 0x90, 0xfe, 0xc4, 0x01, 0x5a, 0x97,  0x61, 0xa6, 0x3d, 0x40, 0x0b, 0x58, 0xe6, 0x3d,
    ],
    [
        0x4d, 0xd1, 0xb2, 0x0f, 0x28, 0xbd, 0xe4, 0x78,  0xf6, 0x4a, 0x0f, 0x93, 0x8b, 0x17, 0xd1, 0xa4,
        0x3a, 0xec, 0xc9, 0x35, 0x93, 0x56, 0x7e, 0xcb,  0x55, 0x20, 0xa0, 0xfe, 0x6c, 0x89, 0x17, 0x62,
        0x17, 0x62, 0x4b, 0xb1, 0xb4, 0xde, 0xd1, 0x87,  0xc9, 0x14, 0x3c, 0x4a, 0x7e, 0xa8, 0xe2, 0x7d,
        0xa0, 0x9f, 0xf6, 0x5c, 0x6a, 0x09, 0x8d, 0xf0,  0x0f, 0xe3, 0x53, 0x25, 0x95, 0x36, 0x28, 0xcb,
    ],
];

/// GRF shuffle-decrypt substitution table.
fn build_shuffle_table() -> [u8; 256] {
    let mut out = [0u8; 256];
    for (i, v) in out.iter_mut().enumerate() {
        *v = i as u8;
    }
    let swaps: [(u8, u8); 7] = [
        (0x00, 0x2b), (0x6c, 0x80), (0x01, 0x68),
        (0x48, 0x77), (0x60, 0xff), (0xb9, 0xc0),
        (0xfe, 0xeb),
    ];
    for (a, b) in swaps {
        out[a as usize] = b;
        out[b as usize] = a;
    }
    out
}

// ---------------------------------------------------------------------------
// Block operations (each operates on a mutable 8-byte slice)
// ---------------------------------------------------------------------------

const MASK: [u8; 8] = [0x80, 0x40, 0x20, 0x10, 0x08, 0x04, 0x02, 0x01];

fn initial_permutation(b: &mut [u8; 8]) {
    let mut tmp = [0u8; 8];
    for (i, &table_val) in IP_TABLE.iter().enumerate() {
        let j = (table_val - 1) as usize;
        if b[(j >> 3) & 7] & MASK[j & 7] != 0 {
            tmp[(i >> 3) & 7] |= MASK[i & 7];
        }
    }
    *b = tmp;
}

fn final_permutation(b: &mut [u8; 8]) {
    let mut tmp = [0u8; 8];
    for (i, &table_val) in FP_TABLE.iter().enumerate() {
        let j = (table_val - 1) as usize;
        if b[(j >> 3) & 7] & MASK[j & 7] != 0 {
            tmp[(i >> 3) & 7] |= MASK[i & 7];
        }
    }
    *b = tmp;
}

fn expansion(b: &mut [u8; 8]) {
    let tmp = [
        ((b[7] << 5) | (b[4] >> 3)) & 0x3f,
        ((b[4] << 1) | (b[5] >> 7)) & 0x3f,
        ((b[4] << 5) | (b[5] >> 3)) & 0x3f,
        ((b[5] << 1) | (b[6] >> 7)) & 0x3f,
        ((b[5] << 5) | (b[6] >> 3)) & 0x3f,
        ((b[6] << 1) | (b[7] >> 7)) & 0x3f,
        ((b[6] << 5) | (b[7] >> 3)) & 0x3f,
        ((b[7] << 1) | (b[4] >> 7)) & 0x3f,
    ];
    *b = tmp;
}

fn substitution_box(b: &mut [u8; 8]) {
    let mut tmp = [0u8; 8];
    for i in 0..4 {
        tmp[i] = (SBOX[i][b[i * 2] as usize] & 0xf0)
            | (SBOX[i][b[i * 2 + 1] as usize] & 0x0f);
    }
    *b = tmp;
}

fn transposition(b: &mut [u8; 8]) {
    let mut tmp = [0u8; 8];
    for (i, &table_val) in P_TABLE.iter().enumerate() {
        let j = (table_val - 1) as usize;
        if b[j >> 3] & MASK[j & 7] != 0 {
            tmp[(i >> 3) + 4] |= MASK[i & 7];
        }
    }
    *b = tmp;
}

fn round_function(b: &mut [u8; 8]) {
    let mut tmp = *b;
    expansion(&mut tmp);
    substitution_box(&mut tmp);
    transposition(&mut tmp);
    b[0] ^= tmp[4];
    b[1] ^= tmp[5];
    b[2] ^= tmp[6];
    b[3] ^= tmp[7];
}

fn decrypt_block(b: &mut [u8; 8]) {
    initial_permutation(b);
    round_function(b);
    final_permutation(b);
}

fn shuffle_dec(b: &mut [u8; 8], table: &[u8; 256]) {
    let tmp = [
        b[3], b[4], b[6], b[0],
        b[1], b[2], b[5], table[b[7] as usize],
    ];
    *b = tmp;
}

// ---------------------------------------------------------------------------
// Public decrypt functions
// ---------------------------------------------------------------------------

fn block_at(buf: &mut [u8], index: usize) -> &mut [u8; 8] {
    let start = index * 8;
    <&mut [u8; 8]>::try_from(&mut buf[start..start + 8]).unwrap()
}

/// Decrypt a fully encrypted entry (ENCRYPT_MIXED, type 0x02).
pub fn decode_full(buf: &mut [u8], length_aligned: usize, pack_size: usize) {
    let nblocks = length_aligned >> 3;
    let shuffle_table = build_shuffle_table();

    // Determine inter-block cycle from digit count of pack_size.
    let digits = pack_size.to_string().len();
    let cycle = match digits {
        d if d < 3 => 1,
        d if d < 5 => d + 1,
        d if d < 7 => d + 9,
        d => d + 15,
    };

    // First 20 blocks: full DES.
    let full_blocks = nblocks.min(20);
    for i in 0..full_blocks {
        decrypt_block(block_at(buf, i));
    }

    // Remaining blocks: cycle-based DES + shuffle.
    let mut j = 0usize;
    for i in 20..nblocks {
        if i % cycle == 0 {
            decrypt_block(block_at(buf, i));
        } else {
            if j == 7 {
                shuffle_dec(block_at(buf, i), &shuffle_table);
                j = 0;
                continue;
            }
            j += 1;
        }
    }
}

/// Decrypt a header-only encrypted entry (ENCRYPT_HEADER, type 0x04).
pub fn decode_header(buf: &mut [u8], length_aligned: usize) {
    let nblocks = (length_aligned >> 3).min(20);
    for i in 0..nblocks {
        decrypt_block(block_at(buf, i));
    }
}
