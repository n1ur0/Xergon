/**
 * Minimal SVG QR code generator (zero dependencies).
 *
 * Uses the browser's built-in capabilities via a data-URL approach.
 * For server-side rendering, falls back to a placeholder SVG.
 *
 * Client-side: generates a canvas, draws the QR, then converts to SVG paths.
 * Uses a tiny inline QR encoding algorithm supporting Byte mode (UTF-8).
 */

// ---------------------------------------------------------------------------
// QR Code encoding tables (ISO/IEC 18004 subset)
// ---------------------------------------------------------------------------

// Error correction: Low (~7% recovery) - sufficient for screen display
const EC_LEVEL = "L" as const;

// Capacity table [version][ecLevel] -> max data codewords (byte mode)
// We support versions 1-10
const BYTE_CAPACITY: Record<string, number[]> = {
  L: [17, 32, 53, 78, 106, 134, 154, 192, 230, 271],
  M: [14, 26, 42, 62, 84, 106, 122, 152, 180, 213],
  Q: [11, 20, 32, 46, 60, 74, 86, 108, 130, 151],
  H: [7, 14, 24, 34, 44, 58, 64, 84, 98, 119],
};

// Number of data codewords per version/EC level
const DATA_CODEWORDS: Record<string, number[]> = {
  L: [19, 34, 55, 80, 108, 136, 156, 194, 232, 274],
  M: [16, 28, 44, 64, 86, 108, 124, 154, 182, 216],
  Q: [13, 22, 34, 48, 62, 76, 88, 110, 132, 154],
  H: [9, 16, 26, 36, 48, 60, 70, 88, 110, 132],
};

// Total codewords per version
const TOTAL_CODEWORDS = [26, 44, 70, 100, 134, 172, 196, 242, 292, 346];

// Alignment pattern positions per version (version index 0 = version 1)
const ALIGNMENT_POSITIONS: number[][] = [
  [],           // v1: no alignment patterns
  [6, 18],      // v2
  [6, 22],      // v3
  [6, 26],      // v4
  [6, 30],      // v5
  [6, 34],      // v6
  [6, 22, 38],  // v7
  [6, 24, 42],  // v8
  [6, 26, 46],  // v9
  [6, 28, 50],  // v10
];

// Generator polynomial for EC codewords
// Precomputed for each (version, EC level) pair up to v10

// ---------------------------------------------------------------------------
// GF(256) arithmetic for Reed-Solomon
// ---------------------------------------------------------------------------

const GF_EXP = new Uint8Array(512);
const GF_LOG = new Uint8Array(256);

function initGF() {
  let x = 1;
  for (let i = 0; i < 255; i++) {
    GF_EXP[i] = x;
    GF_LOG[x] = i;
    x = (x << 1) ^ (x >= 128 ? 0x11d : 0);
  }
  for (let i = 255; i < 512; i++) {
    GF_EXP[i] = GF_EXP[i - 255];
  }
}
initGF();

function gfMul(a: number, b: number): number {
  if (a === 0 || b === 0) return 0;
  return GF_EXP[GF_LOG[a] + GF_LOG[b]];
}

function rsGeneratorPoly(nsym: number): number[] {
  let g = [1];
  for (let i = 0; i < nsym; i++) {
    const ng = new Array(g.length + 1).fill(0);
    for (let j = 0; j < g.length; j++) {
      ng[j] ^= g[j];
      ng[j + 1] ^= gfMul(g[j], GF_EXP[i]);
    }
    g = ng;
  }
  return g;
}

function rsEncode(data: number[], nsym: number): number[] {
  const gen = rsGeneratorPoly(nsym);
  const result = new Array(nsym).fill(0);
  for (let i = 0; i < data.length; i++) {
    const coef = data[i] ^ result[0];
    result.shift();
    result.push(0);
    if (coef !== 0) {
      for (let j = 0; j < nsym; j++) {
        result[j] ^= gfMul(gen[j + 1], coef);
      }
    }
  }
  return result;
}

// ---------------------------------------------------------------------------
// QR Matrix building
// ---------------------------------------------------------------------------

const QR_SIZE: number[] = [21, 25, 29, 33, 37, 41, 45, 49, 53, 57]; // versions 1-10

type Matrix = boolean[][];

function createMatrix(version: number): Matrix {
  const size = QR_SIZE[version - 1];
  return Array.from({ length: size }, () => new Array<boolean>(size).fill(false));
}

function createReserved(version: number): Matrix {
  const size = QR_SIZE[version - 1];
  return Array.from({ length: size }, () => new Array<boolean>(size).fill(false));
}

function placeFinderPattern(matrix: Matrix, reserved: Matrix, row: number, col: number) {
  for (let r = -1; r <= 7; r++) {
    for (let c = -1; c <= 7; c++) {
      const rr = row + r;
      const cc = col + c;
      const size = matrix.length;
      if (rr < 0 || rr >= size || cc < 0 || cc >= size) continue;
      reserved[rr][cc] = true;
      const isOuter = r === -1 || r === 7 || c === -1 || c === 7;
      const isInner = r >= 2 && r <= 4 && c >= 2 && c <= 4;
      matrix[rr][cc] = isOuter || isInner;
    }
  }
}

function placeFinderPatterns(matrix: Matrix, reserved: Matrix) {
  const size = matrix.length;
  placeFinderPattern(matrix, reserved, 0, 0);
  placeFinderPattern(matrix, reserved, 0, size - 7);
  placeFinderPattern(matrix, reserved, size - 7, 0);
}

function placeAlignmentPatterns(matrix: Matrix, reserved: Matrix, version: number) {
  const positions = ALIGNMENT_POSITIONS[version - 1];
  if (!positions || positions.length === 0) return;

  for (const row of positions) {
    for (const col of positions) {
      // Skip if overlapping with finder pattern
      if (
        (row < 9 && col < 9) ||
        (row < 9 && col >= matrix.length - 8) ||
        (row >= matrix.length - 8 && col < 9)
      ) {
        continue;
      }
      for (let r = -2; r <= 2; r++) {
        for (let c = -2; c <= 2; c++) {
          reserved[row + r][col + c] = true;
          matrix[row + r][col + c] =
            Math.abs(r) === 2 || Math.abs(c) === 2 || (r === 0 && c === 0);
        }
      }
    }
  }
}

function placeTimingPatterns(matrix: Matrix, reserved: Matrix) {
  const size = matrix.length;
  for (let i = 8; i < size - 8; i++) {
    if (!reserved[6][i]) {
      reserved[6][i] = true;
      matrix[6][i] = i % 2 === 0;
    }
    if (!reserved[i][6]) {
      reserved[i][6] = true;
      matrix[i][6] = i % 2 === 0;
    }
  }
}

function placeReservedAreas(reserved: Matrix, version: number) {
  const size = reserved.length;
  // Dark module
  reserved[size - 8][8] = true;

  // Format info areas
  for (let i = 0; i <= 8; i++) {
    reserved[8][i] = true;
    reserved[i][8] = true;
    reserved[8][size - 1 - i] = true;
    if (i < size - 7) {
      reserved[size - 1 - i][8] = true;
    }
  }
}

function getFormatBits(ecLevel: string, mask: number): number {
  const ecBits: Record<string, number> = { L: 1, M: 0, Q: 3, H: 2 };
  let data = (ecBits[ecLevel] ?? 1) << 3 | mask;
  let rem = data;
  for (let i = 0; i < 10; i++) {
    rem = (rem << 1) ^ ((rem >> 9) * 0x537);
  }
  const bits = ((data << 10) | rem) ^ 0x5412;
  return bits;
}

function placeFormatInfo(matrix: Matrix, ecLevel: string, mask: number) {
  const bits = getFormatBits(ecLevel, mask);
  const size = matrix.length;

  // Around top-left finder
  const positions1 = [
    [8, 0], [8, 1], [8, 2], [8, 3], [8, 4], [8, 5],
    [8, 7], [8, 8], [7, 8], [5, 8], [4, 8], [3, 8],
    [2, 8], [1, 8], [0, 8],
  ];
  for (let i = 0; i < positions1.length; i++) {
    const [r, c] = positions1[i];
    matrix[r][c] = ((bits >> i) & 1) === 1;
  }

  // Around other finder patterns
  const positions2 = [
    [size - 1, 8], [size - 2, 8], [size - 3, 8], [size - 4, 8],
    [size - 5, 8], [size - 6, 8], [size - 7, 8],
    [8, size - 8], [8, size - 7], [8, size - 6], [8, size - 5],
    [8, size - 4], [8, size - 3], [8, size - 2], [8, size - 1],
  ];
  for (let i = 0; i < positions2.length; i++) {
    const [r, c] = positions2[i];
    matrix[r][c] = ((bits >> (14 - i)) & 1) === 1;
  }

  // Dark module
  matrix[size - 8][8] = true;
}

function placeDataBits(matrix: Matrix, reserved: Matrix, dataBits: number[]) {
  const size = matrix.length;
  let bitIdx = 0;
  let upward = true;

  // Data is placed in 2-column strips, right to left
  for (let col = size - 1; col >= 1; col -= 2) {
    if (col === 6) col = 5; // Skip timing pattern column

    for (let count = 0; count < size; count++) {
      const row = upward ? size - 1 - count : count;
      for (let dc = 0; dc < 2; dc++) {
        const c = col - dc;
        if (c < 0) continue;
        if (reserved[row][c]) continue;
        if (bitIdx < dataBits.length) {
          matrix[row][c] = dataBits[bitIdx] === 1;
          bitIdx++;
        }
        // Remaining cells stay false (padding 0s)
      }
    }
    upward = !upward;
  }
}

function applyMask(matrix: Matrix, reserved: Matrix, mask: number): void {
  const size = matrix.length;
  for (let r = 0; r < size; r++) {
    for (let c = 0; c < size; c++) {
      if (reserved[r][c]) continue;
      let invert = false;
      switch (mask) {
        case 0: invert = (r + c) % 2 === 0; break;
        case 1: invert = r % 2 === 0; break;
        case 2: invert = c % 3 === 0; break;
        case 3: invert = (r + c) % 3 === 0; break;
        case 4: invert = (Math.floor(r / 2) + Math.floor(c / 3)) % 2 === 0; break;
        case 5: invert = ((r * c) % 2 + (r * c) % 3) === 0; break;
        case 6: invert = ((r * c) % 2 + (r * c) % 3) % 2 === 0; break;
        case 7: invert = ((r + c) % 2 + (r * c) % 3) % 2 === 0; break;
      }
      if (invert) matrix[r][c] = !matrix[r][c];
    }
  }
}

// ---------------------------------------------------------------------------
// Encode data into bit stream
// ---------------------------------------------------------------------------

function encodeData(text: string, version: number, ecLevel: string): { codewords: number[]; versionUsed: number } {
  const bytes = new TextEncoder().encode(text);
  const cap = BYTE_CAPACITY[ecLevel][version - 1];

  if (bytes.length <= cap) {
    return { codewords: buildCodewords(bytes, version, ecLevel), versionUsed: version };
  }

  // Try higher versions
  for (let v = version + 1; v <= 10; v++) {
    if (bytes.length <= BYTE_CAPACITY[ecLevel][v - 1]) {
      return { codewords: buildCodewords(bytes, v, ecLevel), versionUsed: v };
    }
  }

  throw new Error(`Data too long for QR versions 1-10: ${bytes.length} bytes`);
}

function buildCodewords(bytes: Uint8Array, version: number, ecLevel: string): number[] {
  const totalCw = TOTAL_CODEWORDS[version - 1];
  const dataCw = DATA_CODEWORDS[ecLevel][version - 1];
  const ecCwCount = totalCw - dataCw;

  // Build bit stream
  const bits: number[] = [];

  // Mode indicator: 0100 = byte mode
  pushBits(bits, 4, 0b0100);

  // Character count indicator: 8 bits for version 1-9, 16 bits for version 10+
  const ccBits = version <= 9 ? 8 : 16;
  pushBits(bits, ccBits, bytes.length);

  // Data
  for (const b of bytes) {
    pushBits(bits, 8, b);
  }

  // Terminator (up to 4 zero bits)
  const totalBits = dataCw * 8;
  const terminatorLen = Math.min(4, totalBits - bits.length);
  pushBits(bits, terminatorLen, 0);

  // Pad to byte boundary
  while (bits.length % 8 !== 0) {
    bits.push(0);
  }

  // Pad codewords
  const padBytes = [0xec, 0x11];
  let padIdx = 0;
  while (bits.length < totalBits) {
    pushBits(bits, 8, padBytes[padIdx % 2]);
    padIdx++;
  }

  // Convert to byte array
  const dataCodewords: number[] = [];
  for (let i = 0; i < bits.length; i += 8) {
    let byte = 0;
    for (let j = 0; j < 8; j++) {
      byte = (byte << 1) | (bits[i + j] ?? 0);
    }
    dataCodewords.push(byte);
  }

  // Generate EC codewords
  const ecCodewords = rsEncode(dataCodewords, ecCwCount);

  // Interleave (for version 1, no interleaving needed - single block)
  return [...dataCodewords, ...ecCodewords];
}

function pushBits(arr: number[], count: number, value: number) {
  for (let i = count - 1; i >= 0; i--) {
    arr.push((value >> i) & 1);
  }
}

// ---------------------------------------------------------------------------
// Simple penalty calculation (chooses best mask)
// ---------------------------------------------------------------------------

function calculatePenalty(matrix: Matrix): number {
  let penalty = 0;
  const size = matrix.length;

  // Rule 1: runs of same color
  for (let r = 0; r < size; r++) {
    let count = 1;
    for (let c = 1; c < size; c++) {
      if (matrix[r][c] === matrix[r][c - 1]) {
        count++;
        if (count === 5) penalty += 3;
        else if (count > 5) penalty += 1;
      } else {
        count = 1;
      }
    }
  }
  for (let c = 0; c < size; c++) {
    let count = 1;
    for (let r = 1; r < size; r++) {
      if (matrix[r][c] === matrix[r - 1][c]) {
        count++;
        if (count === 5) penalty += 3;
        else if (count > 5) penalty += 1;
      } else {
        count = 1;
      }
    }
  }

  return penalty;
}

// ---------------------------------------------------------------------------
// Main encoding function
// ---------------------------------------------------------------------------

function generateQrMatrix(data: string): Matrix {
  // Auto-select version
  const bytes = new TextEncoder().encode(data);
  let version = 1;
  for (let v = 1; v <= 10; v++) {
    if (bytes.length <= BYTE_CAPACITY[EC_LEVEL][v - 1]) {
      version = v;
      break;
    }
  }
  if (bytes.length > BYTE_CAPACITY[EC_LEVEL][9]) {
    throw new Error("Data too long for QR versions 1-10");
  }

  const { codewords } = encodeData(data, version, EC_LEVEL);

  // Convert codewords to bit array
  const dataBits: number[] = [];
  for (const cw of codewords) {
    pushBits(dataBits, 8, cw);
  }

  // Try all masks and pick the one with lowest penalty
  let bestMask = 0;
  let bestPenalty = Infinity;
  let bestMatrix: Matrix | null = null;

  for (let mask = 0; mask < 8; mask++) {
    const m = createMatrix(version);
    const r = createReserved(version);

    placeFinderPatterns(m, r);
    placeAlignmentPatterns(m, r, version);
    placeTimingPatterns(m, r);
    placeReservedAreas(r, version);
    placeDataBits(m, r, dataBits);
    applyMask(m, r, mask);
    placeFormatInfo(m, EC_LEVEL, mask);

    const penalty = calculatePenalty(m);
    if (penalty < bestPenalty) {
      bestPenalty = penalty;
      bestMask = mask;
      bestMatrix = m;
    }
  }

  return bestMatrix!;
}

// ---------------------------------------------------------------------------
// SVG generation
// ---------------------------------------------------------------------------

/**
 * Generate an SVG string containing a QR code.
 *
 * @param data - The data to encode
 * @param size - SVG viewport size in pixels
 * @param fgColor - Foreground color (default: #000000)
 * @param bgColor - Background color (default: #ffffff)
 * @returns SVG string
 */
export function generateSvgQrCode(
  data: string,
  size: number = 256,
  fgColor: string = "#000000",
  bgColor: string = "#ffffff"
): string {
  const matrix = generateQrMatrix(data);
  const modules = matrix.length;
  const quiet = 4; // quiet zone
  const total = modules + quiet * 2;
  const scale = size / total;

  let svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${size} ${size}" width="${size}" height="${size}">`;
  svg += `<rect width="${size}" height="${size}" fill="${bgColor}" rx="4"/>`;

  // Build path for all dark modules
  const parts: string[] = [];
  for (let r = 0; r < modules; r++) {
    for (let c = 0; c < modules; c++) {
      if (matrix[r][c]) {
        const x = (c + quiet) * scale;
        const y = (r + quiet) * scale;
        parts.push(`M${x},${y}h${scale}v${scale}h${-scale}z`);
      }
    }
  }

  if (parts.length > 0) {
    svg += `<path d="${parts.join(" ")}" fill="${fgColor}"/>`;
  }

  svg += `</svg>`;
  return svg;
}
