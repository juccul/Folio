//! Handwritten numeric answer stroke synthesis using curated real-human glyph archetypes.
//!
//! Synthesizes natural multi-stroke `InkStroke`s that match the user's pen style, glyph height,
//! midline, slant, and pressure profile, with subtle deterministic per-character variation.
use folio_document::*;
use folio_ink::StrokeBuilder;

#[derive(Clone, Copy)]
struct GlyphVariant {
    advance: f32,
    strokes: &'static [&'static [[f32; 2]]],
}

/// Writing style extracted from the user's equation strokes.
#[derive(Clone, Debug)]
pub struct HandwritingStyle {
    pub pen: PenStyle,
    pub glyph_height: f32,
    pub midline_y: f32,
    pub slant: f32,
    pub pressure: f32,
}

/// Write-on timing for a single synthesized answer stroke.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StrokeReveal {
    pub id: Id,
    pub start_ms: u32,
    pub duration_ms: u32,
}

/// Synthesized handwritten answer strokes paired with sequential write-on timings.
pub struct SynthesizedAnswer {
    pub strokes: Vec<InkStroke>,
    pub reveals: Vec<StrokeReveal>,
    pub write_end_ms: u32,
}

const S_ZERO_0_0: &[[f32; 2]] = &[[0.3, 0.254], [0.175, 0.347], [0.079, 0.462], [0.031, 0.602], [0.04, 0.748], [0.107, 0.873], [0.221, 0.945], [0.353, 0.944], [0.469, 0.869], [0.546, 0.743], [0.578, 0.595], [0.563, 0.443], [0.511, 0.296], [0.43, 0.168], [0.326, 0.081], [0.206, 0.039]];
const V_ZERO_0: GlyphVariant = GlyphVariant { advance: 0.547, strokes: &[S_ZERO_0_0] };
const S_ZERO_1_0: &[[f32; 2]] = &[[0.02, 0.284], [0.012, 0.444], [0.015, 0.604], [0.035, 0.759], [0.083, 0.877], [0.168, 0.913], [0.273, 0.857], [0.38, 0.746], [0.479, 0.618], [0.56, 0.482], [0.602, 0.339], [0.579, 0.201], [0.49, 0.094], [0.356, 0.036], [0.202, 0.029], [0.046, 0.054]];
const V_ZERO_1: GlyphVariant = GlyphVariant { advance: 0.59, strokes: &[S_ZERO_1_0] };
const S_ZERO_2_0: &[[f32; 2]] = &[[0.043, 0.51], [0.062, 0.672], [0.128, 0.816], [0.245, 0.91], [0.377, 0.916], [0.485, 0.824], [0.547, 0.672], [0.559, 0.5], [0.528, 0.328], [0.455, 0.177], [0.342, 0.083], [0.21, 0.083], [0.101, 0.178], [0.04, 0.331], [0.023, 0.506], [0.028, 0.685]];
const V_ZERO_2: GlyphVariant = GlyphVariant { advance: 0.536, strokes: &[S_ZERO_2_0] };
const GLYPH_ZERO: &[GlyphVariant] = &[V_ZERO_0, V_ZERO_1, V_ZERO_2];
const S_ONE_0_0: &[[f32; 2]] = &[[0.0, 0.574], [0.061, 0.489], [0.11, 0.395], [0.148, 0.293], [0.178, 0.191], [0.194, 0.11], [0.19, 0.086], [0.173, 0.137], [0.155, 0.234], [0.144, 0.342], [0.138, 0.452], [0.133, 0.561], [0.129, 0.671], [0.126, 0.781], [0.125, 0.89], [0.124, 1.0]];
const V_ONE_0: GlyphVariant = GlyphVariant { advance: 0.194, strokes: &[S_ONE_0_0] };
const S_ONE_1_0: &[[f32; 2]] = &[[0.0, 0.673], [0.065, 0.572], [0.129, 0.47], [0.188, 0.366], [0.24, 0.258], [0.28, 0.157], [0.297, 0.096], [0.283, 0.111], [0.248, 0.191], [0.209, 0.3], [0.174, 0.414], [0.143, 0.53], [0.117, 0.648], [0.094, 0.765], [0.07, 0.883], [0.044, 1.0]];
const V_ONE_1: GlyphVariant = GlyphVariant { advance: 0.297, strokes: &[S_ONE_1_0] };
const S_ONE_2_0: &[[f32; 2]] = &[[0.0, 0.817], [0.027, 0.698], [0.054, 0.578], [0.082, 0.458], [0.107, 0.338], [0.13, 0.22], [0.154, 0.126], [0.172, 0.099], [0.179, 0.156], [0.173, 0.266], [0.162, 0.388], [0.15, 0.511], [0.141, 0.633], [0.136, 0.756], [0.138, 0.878], [0.145, 1.0]];
const V_ONE_2: GlyphVariant = GlyphVariant { advance: 0.179, strokes: &[S_ONE_2_0] };
const GLYPH_ONE: &[GlyphVariant] = &[V_ONE_0, V_ONE_1, V_ONE_2];
const S_TWO_0_0: &[[f32; 2]] = &[[0.0, 0.083], [0.165, 0.06], [0.308, 0.088], [0.397, 0.187], [0.408, 0.332], [0.354, 0.488], [0.274, 0.636], [0.229, 0.766], [0.272, 0.868], [0.399, 0.927], [0.551, 0.951], [0.654, 0.97], [0.684, 1.0]];
const V_TWO_0: GlyphVariant = GlyphVariant { advance: 0.684, strokes: &[S_TWO_0_0] };
const S_TWO_1_0: &[[f32; 2]] = &[[0.0, 0.143], [0.138, 0.091], [0.273, 0.051], [0.39, 0.056], [0.457, 0.128], [0.454, 0.245], [0.398, 0.373], [0.321, 0.499], [0.247, 0.626], [0.199, 0.755], [0.207, 0.873], [0.285, 0.953], [0.411, 0.985], [0.555, 0.982], [0.702, 0.96], [0.847, 0.931]];
const V_TWO_1: GlyphVariant = GlyphVariant { advance: 0.847, strokes: &[S_TWO_1_0] };
const S_TWO_2_0: &[[f32; 2]] = &[[0.0, 0.09], [0.124, 0.036], [0.255, 0.022], [0.373, 0.064], [0.444, 0.159], [0.456, 0.288], [0.428, 0.427], [0.376, 0.563], [0.312, 0.694], [0.258, 0.817], [0.257, 0.913], [0.336, 0.964], [0.467, 0.975], [0.612, 0.976], [0.758, 0.984], [0.904, 1.0]];
const V_TWO_2: GlyphVariant = GlyphVariant { advance: 0.904, strokes: &[S_TWO_2_0] };
const GLYPH_TWO: &[GlyphVariant] = &[V_TWO_0, V_TWO_1, V_TWO_2];
const S_THREE_0_0: &[[f32; 2]] = &[[0.173, 0.272], [0.256, 0.146], [0.372, 0.069], [0.486, 0.078], [0.526, 0.165], [0.462, 0.279], [0.338, 0.375], [0.244, 0.442], [0.25, 0.493], [0.352, 0.55], [0.469, 0.636], [0.51, 0.752], [0.446, 0.867], [0.311, 0.943], [0.157, 0.962], [0.01, 0.936]];
const V_THREE_0: GlyphVariant = GlyphVariant { advance: 0.516, strokes: &[S_THREE_0_0] };
const S_THREE_1_0: &[[f32; 2]] = &[[0.022, 0.198], [0.096, 0.104], [0.201, 0.044], [0.324, 0.039], [0.417, 0.099], [0.446, 0.208], [0.416, 0.333], [0.373, 0.439], [0.376, 0.518], [0.441, 0.591], [0.514, 0.686], [0.529, 0.798], [0.468, 0.895], [0.352, 0.954], [0.212, 0.983], [0.067, 1.0]];
const V_THREE_1: GlyphVariant = GlyphVariant { advance: 0.507, strokes: &[S_THREE_1_0] };
const S_THREE_2_0: &[[f32; 2]] = &[[0.034, 0.298], [0.078, 0.178], [0.161, 0.088], [0.263, 0.061], [0.328, 0.113], [0.335, 0.218], [0.334, 0.317], [0.384, 0.391], [0.474, 0.47], [0.552, 0.579], [0.579, 0.71], [0.54, 0.834], [0.443, 0.924], [0.32, 0.959], [0.213, 0.928], [0.141, 0.845]];
const V_THREE_2: GlyphVariant = GlyphVariant { advance: 0.545, strokes: &[S_THREE_2_0] };
const GLYPH_THREE: &[GlyphVariant] = &[V_THREE_0, V_THREE_1, V_THREE_2];
const S_FOUR_0_0: &[[f32; 2]] = &[[0.325, 0.0], [0.254, 0.048], [0.197, 0.113], [0.154, 0.193], [0.119, 0.28], [0.084, 0.366], [0.048, 0.452], [0.024, 0.537], [0.033, 0.612], [0.083, 0.663], [0.163, 0.688], [0.255, 0.7], [0.348, 0.708], [0.442, 0.713], [0.536, 0.714], [0.63, 0.714]];
const S_FOUR_0_1: &[[f32; 2]] = &[[0.429, 0.221], [0.384, 0.289], [0.342, 0.359], [0.306, 0.433], [0.277, 0.51], [0.253, 0.589], [0.234, 0.67], [0.225, 0.751], [0.221, 0.834], [0.221, 0.917], [0.221, 1.0]];
const V_FOUR_0: GlyphVariant = GlyphVariant { advance: 0.606, strokes: &[S_FOUR_0_0, S_FOUR_0_1] };
const S_FOUR_1_0: &[[f32; 2]] = &[[0.232, 0.029], [0.154, 0.034], [0.091, 0.067], [0.049, 0.133], [0.026, 0.217], [0.013, 0.306], [0.006, 0.396], [0.005, 0.483], [0.026, 0.557], [0.079, 0.606], [0.157, 0.628], [0.245, 0.629], [0.333, 0.614], [0.42, 0.592], [0.507, 0.571], [0.595, 0.551]];
const S_FOUR_1_1: &[[f32; 2]] = &[[0.479, 0.044], [0.477, 0.15], [0.472, 0.257], [0.464, 0.363], [0.452, 0.469], [0.438, 0.575], [0.423, 0.681], [0.413, 0.787], [0.408, 0.893], [0.406, 1.0]];
const V_FOUR_1: GlyphVariant = GlyphVariant { advance: 0.59, strokes: &[S_FOUR_1_0, S_FOUR_1_1] };
const S_FOUR_2_0: &[[f32; 2]] = &[[0.465, 0.14], [0.452, 0.158], [0.451, 0.2], [0.463, 0.259], [0.479, 0.323], [0.497, 0.386], [0.514, 0.45], [0.53, 0.514], [0.543, 0.578], [0.553, 0.643], [0.562, 0.709], [0.569, 0.775], [0.573, 0.841], [0.572, 0.902], [0.568, 0.945], [0.561, 0.963]];
const S_FOUR_2_1: &[[f32; 2]] = &[[0.078, 0.042], [0.034, 0.06], [0.015, 0.117], [0.018, 0.205], [0.028, 0.301], [0.037, 0.397], [0.045, 0.494], [0.055, 0.59], [0.07, 0.677], [0.098, 0.728], [0.148, 0.727], [0.221, 0.696], [0.309, 0.669], [0.398, 0.66], [0.467, 0.678], [0.51, 0.719]];
const V_FOUR_2: GlyphVariant = GlyphVariant { advance: 0.558, strokes: &[S_FOUR_2_0, S_FOUR_2_1] };
const GLYPH_FOUR: &[GlyphVariant] = &[V_FOUR_0, V_FOUR_1, V_FOUR_2];
const S_FIVE_0_0: &[[f32; 2]] = &[[0.071, 0.0], [0.083, 0.115], [0.088, 0.229], [0.094, 0.338], [0.122, 0.417], [0.191, 0.448], [0.292, 0.441], [0.403, 0.429], [0.506, 0.441], [0.585, 0.494], [0.629, 0.583], [0.634, 0.69], [0.605, 0.797], [0.549, 0.89], [0.466, 0.957], [0.366, 1.0]];
const S_FIVE_0_1: &[[f32; 2]] = &[[0.0, 0.116], [0.051, 0.108], [0.102, 0.099], [0.153, 0.09], [0.204, 0.082], [0.255, 0.073], [0.306, 0.063], [0.357, 0.054]];
const V_FIVE_0: GlyphVariant = GlyphVariant { advance: 0.634, strokes: &[S_FIVE_0_0, S_FIVE_0_1] };
const S_FIVE_1_0: &[[f32; 2]] = &[[0.118, 0.14], [0.165, 0.118], [0.213, 0.097], [0.261, 0.08], [0.311, 0.065], [0.361, 0.051], [0.411, 0.038], [0.459, 0.026], [0.497, 0.016], [0.523, 0.007]];
const S_FIVE_1_1: &[[f32; 2]] = &[[0.088, 0.154], [0.085, 0.259], [0.082, 0.361], [0.097, 0.445], [0.149, 0.494], [0.234, 0.512], [0.331, 0.529], [0.416, 0.572], [0.472, 0.645], [0.487, 0.736], [0.458, 0.824], [0.393, 0.897], [0.306, 0.949], [0.208, 0.98], [0.105, 0.995], [0.0, 1.0]];
const V_FIVE_1: GlyphVariant = GlyphVariant { advance: 0.523, strokes: &[S_FIVE_1_0, S_FIVE_1_1] };
const S_FIVE_2_0: &[[f32; 2]] = &[[0.164, 0.0], [0.151, 0.095], [0.14, 0.19], [0.134, 0.285], [0.136, 0.379], [0.154, 0.468], [0.197, 0.539], [0.268, 0.583], [0.352, 0.612], [0.429, 0.651], [0.474, 0.715], [0.475, 0.794], [0.435, 0.87], [0.369, 0.931], [0.287, 0.973], [0.198, 1.0]];
const S_FIVE_2_1: &[[f32; 2]] = &[[0.0, 0.095], [0.061, 0.085], [0.122, 0.077], [0.184, 0.072], [0.246, 0.07], [0.307, 0.069], [0.369, 0.069], [0.431, 0.069]];
const V_FIVE_2: GlyphVariant = GlyphVariant { advance: 0.475, strokes: &[S_FIVE_2_0, S_FIVE_2_1] };
const GLYPH_FIVE: &[GlyphVariant] = &[V_FIVE_0, V_FIVE_1, V_FIVE_2];
const S_SIX_0_0: &[[f32; 2]] = &[[0.337, 0.0], [0.227, 0.098], [0.135, 0.214], [0.069, 0.349], [0.029, 0.495], [0.016, 0.644], [0.038, 0.788], [0.106, 0.9], [0.213, 0.946], [0.328, 0.91], [0.414, 0.808], [0.448, 0.675], [0.419, 0.56], [0.331, 0.509], [0.219, 0.541], [0.111, 0.631]];
const V_SIX_0: GlyphVariant = GlyphVariant { advance: 0.432, strokes: &[S_SIX_0_0] };
const S_SIX_1_0: &[[f32; 2]] = &[[0.752, 0.066], [0.595, 0.038], [0.447, 0.047], [0.328, 0.118], [0.25, 0.244], [0.206, 0.397], [0.187, 0.558], [0.194, 0.717], [0.241, 0.852], [0.331, 0.922], [0.429, 0.895], [0.472, 0.797], [0.424, 0.693], [0.304, 0.634], [0.154, 0.633], [0.0, 0.669]];
const V_SIX_1: GlyphVariant = GlyphVariant { advance: 0.752, strokes: &[S_SIX_1_0] };
const S_SIX_2_0: &[[f32; 2]] = &[[0.546, 0.074], [0.446, 0.048], [0.339, 0.072], [0.235, 0.15], [0.148, 0.259], [0.08, 0.382], [0.034, 0.515], [0.019, 0.651], [0.044, 0.78], [0.117, 0.883], [0.219, 0.934], [0.312, 0.911], [0.361, 0.817], [0.351, 0.696], [0.285, 0.591], [0.181, 0.513]];
const V_SIX_2: GlyphVariant = GlyphVariant { advance: 0.527, strokes: &[S_SIX_2_0] };
const GLYPH_SIX: &[GlyphVariant] = &[V_SIX_0, V_SIX_1, V_SIX_2];
const S_SEVEN_0_0: &[[f32; 2]] = &[[0.0, 0.015], [0.103, 0.006], [0.207, 0.003], [0.31, 0.008], [0.409, 0.029], [0.494, 0.073], [0.553, 0.146], [0.579, 0.238], [0.574, 0.337], [0.548, 0.436], [0.511, 0.533], [0.469, 0.628], [0.423, 0.721], [0.377, 0.814], [0.33, 0.907], [0.285, 1.0]];
const V_SEVEN_0: GlyphVariant = GlyphVariant { advance: 0.579, strokes: &[S_SEVEN_0_0] };
const S_SEVEN_1_0: &[[f32; 2]] = &[[0.0, 0.018], [0.099, 0.014], [0.199, 0.017], [0.299, 0.022], [0.392, 0.027], [0.46, 0.053], [0.488, 0.115], [0.48, 0.205], [0.456, 0.302], [0.431, 0.4], [0.408, 0.499], [0.388, 0.598], [0.372, 0.698], [0.359, 0.798], [0.351, 0.899], [0.347, 1.0]];
const V_SEVEN_1: GlyphVariant = GlyphVariant { advance: 0.488, strokes: &[S_SEVEN_1_0] };
const S_SEVEN_2_0: &[[f32; 2]] = &[[0.0, 0.076], [0.113, 0.061], [0.226, 0.045], [0.339, 0.028], [0.453, 0.014], [0.563, 0.01], [0.655, 0.036], [0.712, 0.105], [0.733, 0.207], [0.734, 0.32], [0.726, 0.433], [0.713, 0.547], [0.697, 0.66], [0.681, 0.773], [0.665, 0.887], [0.652, 1.0]];
const V_SEVEN_2: GlyphVariant = GlyphVariant { advance: 0.734, strokes: &[S_SEVEN_2_0] };
const GLYPH_SEVEN: &[GlyphVariant] = &[V_SEVEN_0, V_SEVEN_1, V_SEVEN_2];
const S_EIGHT_0_0: &[[f32; 2]] = &[[0.461, 0.233], [0.407, 0.115], [0.296, 0.062], [0.177, 0.1], [0.143, 0.205], [0.213, 0.34], [0.312, 0.494], [0.357, 0.662], [0.313, 0.818], [0.201, 0.907], [0.099, 0.883], [0.083, 0.765], [0.164, 0.617], [0.296, 0.474], [0.439, 0.335], [0.578, 0.192]];
const V_EIGHT_0: GlyphVariant = GlyphVariant { advance: 0.495, strokes: &[S_EIGHT_0_0] };
const S_EIGHT_1_0: &[[f32; 2]] = &[[0.067, 0.015], [0.152, 0.008], [0.236, 0.009], [0.319, 0.022], [0.396, 0.051], [0.456, 0.099], [0.481, 0.163], [0.461, 0.228], [0.405, 0.279], [0.33, 0.313], [0.249, 0.329], [0.167, 0.325], [0.093, 0.297], [0.041, 0.245], [0.022, 0.175], [0.031, 0.098]];
const S_EIGHT_1_1: &[[f32; 2]] = &[[0.129, 0.397], [0.231, 0.433], [0.327, 0.48], [0.411, 0.545], [0.476, 0.627], [0.513, 0.721], [0.509, 0.816], [0.46, 0.899], [0.377, 0.955], [0.28, 0.975], [0.187, 0.952], [0.114, 0.889], [0.076, 0.8], [0.082, 0.702], [0.125, 0.61], [0.191, 0.526]];
const V_EIGHT_1: GlyphVariant = GlyphVariant { advance: 0.491, strokes: &[S_EIGHT_1_0, S_EIGHT_1_1] };
const S_EIGHT_2_0: &[[f32; 2]] = &[[0.247, 0.0], [0.182, 0.027], [0.128, 0.066], [0.093, 0.122], [0.083, 0.187], [0.101, 0.247], [0.146, 0.289], [0.208, 0.305], [0.277, 0.299], [0.344, 0.277], [0.402, 0.239], [0.448, 0.188], [0.475, 0.127], [0.473, 0.069], [0.438, 0.026], [0.38, 0.002]];
const S_EIGHT_2_1: &[[f32; 2]] = &[[0.272, 0.367], [0.186, 0.436], [0.11, 0.513], [0.053, 0.603], [0.022, 0.705], [0.022, 0.808], [0.059, 0.898], [0.133, 0.959], [0.231, 0.985], [0.336, 0.977], [0.431, 0.937], [0.503, 0.865], [0.541, 0.772], [0.539, 0.67], [0.503, 0.572], [0.446, 0.479]];
const V_EIGHT_2: GlyphVariant = GlyphVariant { advance: 0.519, strokes: &[S_EIGHT_2_0, S_EIGHT_2_1] };
const GLYPH_EIGHT: &[GlyphVariant] = &[V_EIGHT_0, V_EIGHT_1, V_EIGHT_2];
const S_NINE_0_0: &[[f32; 2]] = &[[0.084, 0.953], [0.217, 0.926], [0.332, 0.847], [0.424, 0.721], [0.494, 0.575], [0.53, 0.421], [0.521, 0.266], [0.461, 0.133], [0.353, 0.058], [0.221, 0.065], [0.107, 0.145], [0.058, 0.265], [0.097, 0.384], [0.21, 0.469], [0.361, 0.514], [0.522, 0.535]];
const V_NINE_0: GlyphVariant = GlyphVariant { advance: 0.472, strokes: &[S_NINE_0_0] };
const S_NINE_1_0: &[[f32; 2]] = &[[0.564, 0.282], [0.462, 0.196], [0.357, 0.114], [0.246, 0.055], [0.137, 0.044], [0.065, 0.095], [0.063, 0.184], [0.131, 0.258], [0.235, 0.29], [0.329, 0.31], [0.381, 0.37], [0.395, 0.479], [0.394, 0.609], [0.394, 0.743], [0.405, 0.873], [0.43, 1.0]];
const V_NINE_1: GlyphVariant = GlyphVariant { advance: 0.501, strokes: &[S_NINE_1_0] };
const S_NINE_2_0: &[[f32; 2]] = &[[0.537, 0.06], [0.377, 0.063], [0.232, 0.126], [0.127, 0.247], [0.1, 0.381], [0.172, 0.468], [0.306, 0.468], [0.433, 0.396], [0.503, 0.333], [0.517, 0.363], [0.509, 0.492], [0.492, 0.665], [0.44, 0.825], [0.332, 0.932], [0.176, 0.966], [0.0, 0.947]];
const V_NINE_2: GlyphVariant = GlyphVariant { advance: 0.537, strokes: &[S_NINE_2_0] };
const GLYPH_NINE: &[GlyphVariant] = &[V_NINE_0, V_NINE_1, V_NINE_2];
const S_DOT_0_0: &[[f32; 2]] = &[[0.04, 0.89], [0.059, 0.917], [0.054, 0.929], [0.03, 0.92]];
const V_DOT_0: GlyphVariant = GlyphVariant { advance: 0.14, strokes: &[S_DOT_0_0] };
const S_DOT_1_0: &[[f32; 2]] = &[[0.04, 0.89], [0.059, 0.917], [0.054, 0.929], [0.03, 0.92]];
const V_DOT_1: GlyphVariant = GlyphVariant { advance: 0.14, strokes: &[S_DOT_1_0] };
const S_DOT_2_0: &[[f32; 2]] = &[[0.04, 0.89], [0.059, 0.917], [0.054, 0.929], [0.03, 0.92]];
const V_DOT_2: GlyphVariant = GlyphVariant { advance: 0.14, strokes: &[S_DOT_2_0] };
const GLYPH_DOT: &[GlyphVariant] = &[V_DOT_0, V_DOT_1, V_DOT_2];
const S_MINUS_0_0: &[[f32; 2]] = &[[0.0, 0.531], [0.103, 0.511], [0.207, 0.492], [0.31, 0.479], [0.415, 0.474], [0.52, 0.473]];
const V_MINUS_0: GlyphVariant = GlyphVariant { advance: 0.52, strokes: &[S_MINUS_0_0] };
const S_MINUS_1_0: &[[f32; 2]] = &[[0.0, 0.461], [0.103, 0.477], [0.207, 0.494], [0.311, 0.509], [0.415, 0.524], [0.52, 0.539]];
const V_MINUS_1: GlyphVariant = GlyphVariant { advance: 0.52, strokes: &[S_MINUS_1_0] };
const S_MINUS_2_0: &[[f32; 2]] = &[[0.0, 0.474], [0.104, 0.482], [0.209, 0.491], [0.313, 0.499], [0.417, 0.511], [0.52, 0.526]];
const V_MINUS_2: GlyphVariant = GlyphVariant { advance: 0.52, strokes: &[S_MINUS_2_0] };
const GLYPH_MINUS: &[GlyphVariant] = &[V_MINUS_0, V_MINUS_1, V_MINUS_2];
const S_PLUS_0_0: &[[f32; 2]] = &[[0.467, 0.0], [0.463, 0.083], [0.463, 0.167], [0.466, 0.251], [0.467, 0.335], [0.467, 0.419], [0.467, 0.503], [0.465, 0.587], [0.461, 0.67], [0.458, 0.754], [0.46, 0.838], [0.473, 0.92], [0.494, 1.0]];
const S_PLUS_0_1: &[[f32; 2]] = &[[0.0, 0.455], [0.106, 0.455], [0.213, 0.455], [0.319, 0.455], [0.425, 0.455], [0.532, 0.455], [0.638, 0.455], [0.745, 0.455], [0.851, 0.455]];
const V_PLUS_0: GlyphVariant = GlyphVariant { advance: 0.851, strokes: &[S_PLUS_0_0, S_PLUS_0_1] };
const S_PLUS_1_0: &[[f32; 2]] = &[[0.523, 0.0], [0.539, 0.1], [0.548, 0.2], [0.549, 0.301], [0.543, 0.402], [0.533, 0.503], [0.52, 0.603], [0.51, 0.704], [0.508, 0.804], [0.521, 0.903], [0.546, 1.0]];
const S_PLUS_1_1: &[[f32; 2]] = &[[0.0, 0.443], [0.112, 0.435], [0.224, 0.431], [0.337, 0.428], [0.45, 0.424], [0.563, 0.42], [0.676, 0.42], [0.788, 0.423], [0.901, 0.43], [1.01, 0.441], [1.13, 0.455]];
const V_PLUS_1: GlyphVariant = GlyphVariant { advance: 1.13, strokes: &[S_PLUS_1_0, S_PLUS_1_1] };
const S_PLUS_2_0: &[[f32; 2]] = &[[0.159, 0.0], [0.185, 0.073], [0.205, 0.148], [0.219, 0.224], [0.227, 0.302], [0.232, 0.38], [0.234, 0.458], [0.233, 0.536], [0.227, 0.613], [0.218, 0.691], [0.209, 0.768], [0.205, 0.846], [0.207, 0.923], [0.216, 1.0]];
const S_PLUS_2_1: &[[f32; 2]] = &[[0.0, 0.572], [0.055, 0.528], [0.119, 0.501], [0.193, 0.49], [0.269, 0.484], [0.345, 0.48], [0.422, 0.477], [0.499, 0.474], [0.575, 0.472], [0.652, 0.47], [0.728, 0.468]];
const V_PLUS_2: GlyphVariant = GlyphVariant { advance: 0.728, strokes: &[S_PLUS_2_0, S_PLUS_2_1] };
const GLYPH_PLUS: &[GlyphVariant] = &[V_PLUS_0, V_PLUS_1, V_PLUS_2];
const S_E_0_0: &[[f32; 2]] = &[[0.366, 0.381], [0.511, 0.349], [0.62, 0.28], [0.654, 0.176], [0.596, 0.08], [0.473, 0.028], [0.326, 0.031], [0.19, 0.085], [0.088, 0.187], [0.033, 0.322], [0.025, 0.472], [0.06, 0.621], [0.133, 0.754], [0.239, 0.863], [0.37, 0.942], [0.515, 1.0]];
const V_E_0: GlyphVariant = GlyphVariant { advance: 0.629, strokes: &[S_E_0_0] };
const S_E_1_0: &[[f32; 2]] = &[[0.0, 0.658], [0.165, 0.59], [0.327, 0.516], [0.478, 0.426], [0.601, 0.311], [0.662, 0.182], [0.629, 0.086], [0.516, 0.073], [0.373, 0.139], [0.237, 0.249], [0.126, 0.383], [0.055, 0.536], [0.042, 0.698], [0.1, 0.843], [0.219, 0.943], [0.377, 1.0]];
const V_E_1: GlyphVariant = GlyphVariant { advance: 0.662, strokes: &[S_E_1_0] };
const S_E_2_0: &[[f32; 2]] = &[[0.0, 0.503], [0.164, 0.464], [0.317, 0.402], [0.444, 0.304], [0.515, 0.185], [0.499, 0.094], [0.409, 0.089], [0.297, 0.173], [0.211, 0.309], [0.162, 0.467], [0.153, 0.632], [0.189, 0.787], [0.276, 0.904], [0.406, 0.95], [0.557, 0.925], [0.71, 0.858]];
const V_E_2: GlyphVariant = GlyphVariant { advance: 0.71, strokes: &[S_E_2_0] };
const GLYPH_E: &[GlyphVariant] = &[V_E_0, V_E_1, V_E_2];
const S_COMMA_0_0: &[[f32; 2]] = &[[0.352, 0.0], [0.32, 0.101], [0.289, 0.203], [0.258, 0.305], [0.229, 0.407], [0.197, 0.508], [0.164, 0.61], [0.131, 0.71], [0.093, 0.809], [0.049, 0.906], [0.0, 1.0]];
const V_COMMA_0: GlyphVariant = GlyphVariant { advance: 0.352, strokes: &[S_COMMA_0_0] };
const S_COMMA_1_0: &[[f32; 2]] = &[[0.327, 0.0], [0.312, 0.069], [0.297, 0.137], [0.279, 0.205], [0.259, 0.272], [0.236, 0.339], [0.212, 0.405], [0.187, 0.471], [0.162, 0.536], [0.136, 0.601], [0.111, 0.667], [0.087, 0.733], [0.064, 0.799], [0.041, 0.866], [0.02, 0.933], [0.0, 1.0]];
const V_COMMA_1: GlyphVariant = GlyphVariant { advance: 0.327, strokes: &[S_COMMA_1_0] };
const S_COMMA_2_0: &[[f32; 2]] = &[[0.315, 0.0], [0.284, 0.147], [0.247, 0.292], [0.206, 0.437], [0.162, 0.581], [0.115, 0.723], [0.061, 0.863], [0.0, 1.0]];
const V_COMMA_2: GlyphVariant = GlyphVariant { advance: 0.315, strokes: &[S_COMMA_2_0] };
const GLYPH_COMMA: &[GlyphVariant] = &[V_COMMA_0, V_COMMA_1, V_COMMA_2];
const S_LET_X_0_0: &[[f32; 2]] = &[[0.208, 0.146], [0.125, 0.127], [0.141, 0.213], [0.177, 0.292], [0.221, 0.368], [0.268, 0.441], [0.316, 0.514], [0.364, 0.587], [0.412, 0.660], [0.468, 0.728], [0.523, 0.795], [0.578, 0.863], [0.633, 0.932], [0.688, 1.000]];
const S_LET_X_0_1: &[[f32; 2]] = &[[0.000, 0.958], [0.048, 0.883], [0.096, 0.808], [0.143, 0.733], [0.189, 0.657], [0.235, 0.581], [0.282, 0.505], [0.331, 0.431], [0.380, 0.357], [0.431, 0.284], [0.483, 0.211], [0.536, 0.140], [0.589, 0.068], [0.646, 0.000]];
const V_LET_X_0: GlyphVariant = GlyphVariant { advance: 0.688, strokes: &[S_LET_X_0_0, S_LET_X_0_1] };
const S_LET_X_1_0: &[[f32; 2]] = &[[0.128, 0.000], [0.169, 0.082], [0.221, 0.157], [0.273, 0.233], [0.320, 0.311], [0.365, 0.390], [0.404, 0.473], [0.439, 0.558], [0.472, 0.644], [0.510, 0.727], [0.557, 0.805], [0.610, 0.881], [0.661, 0.956], [0.682, 0.956]];
const S_LET_X_1_1: &[[f32; 2]] = &[[0.684, 0.141], [0.620, 0.197], [0.571, 0.276], [0.522, 0.356], [0.475, 0.437], [0.432, 0.519], [0.389, 0.602], [0.338, 0.680], [0.277, 0.750], [0.210, 0.816], [0.146, 0.884], [0.087, 0.956], [0.014, 0.988], [0.032, 0.908]];
const V_LET_X_1: GlyphVariant = GlyphVariant { advance: 0.688, strokes: &[S_LET_X_1_0, S_LET_X_1_1] };
const GLYPH_LET_X: &[GlyphVariant] = &[V_LET_X_0, V_LET_X_1];
const S_LET_Y_0_0: &[[f32; 2]] = &[[0.000, 0.062], [0.042, 0.084], [0.082, 0.109], [0.120, 0.137], [0.157, 0.167], [0.192, 0.199], [0.226, 0.232], [0.259, 0.266], [0.293, 0.300], [0.326, 0.333], [0.360, 0.367], [0.391, 0.402], [0.418, 0.441], [0.445, 0.480]];
const S_LET_Y_0_1: &[[f32; 2]] = &[[0.608, 0.000], [0.616, 0.095], [0.613, 0.191], [0.599, 0.286], [0.576, 0.378], [0.548, 0.470], [0.510, 0.557], [0.470, 0.644], [0.424, 0.728], [0.375, 0.809], [0.308, 0.876], [0.232, 0.934], [0.143, 0.968], [0.053, 1.000]];
const V_LET_Y_0: GlyphVariant = GlyphVariant { advance: 0.617, strokes: &[S_LET_Y_0_0, S_LET_Y_0_1] };
const S_LET_Y_1_0: &[[f32; 2]] = &[[0.201, 0.000], [0.215, 0.170], [0.364, 0.233], [0.514, 0.132], [0.526, 0.072], [0.516, 0.251], [0.541, 0.431], [0.572, 0.610], [0.578, 0.792], [0.517, 0.959], [0.345, 1.000], [0.164, 0.982], [0.004, 0.908], [0.128, 0.787]];
const V_LET_Y_1: GlyphVariant = GlyphVariant { advance: 0.579, strokes: &[S_LET_Y_1_0] };
const GLYPH_LET_Y: &[GlyphVariant] = &[V_LET_Y_0, V_LET_Y_1];
const S_LET_Z_0_0: &[[f32; 2]] = &[[0.000, 0.000], [0.170, 0.000], [0.340, 0.005], [0.508, 0.037], [0.642, 0.129], [0.605, 0.282], [0.494, 0.412], [0.367, 0.525], [0.236, 0.633], [0.111, 0.749], [0.005, 0.880], [0.131, 0.970], [0.300, 0.984], [0.469, 1.000]];
const V_LET_Z_0: GlyphVariant = GlyphVariant { advance: 0.656, strokes: &[S_LET_Z_0_0] };
const S_LET_Z_1_0: &[[f32; 2]] = &[[0.091, 0.329], [0.203, 0.163], [0.342, 0.021], [0.511, 0.083], [0.693, 0.070], [0.612, 0.228], [0.497, 0.392], [0.375, 0.551], [0.241, 0.699], [0.109, 0.850], [0.028, 0.987], [0.223, 0.970], [0.412, 0.905], [0.588, 0.812]];
const V_LET_Z_1: GlyphVariant = GlyphVariant { advance: 0.702, strokes: &[S_LET_Z_1_0] };
const GLYPH_LET_Z: &[GlyphVariant] = &[V_LET_Z_0, V_LET_Z_1];
const S_LET_A_0_0: &[[f32; 2]] = &[[0.566, 0.253], [0.548, 0.095], [0.368, 0.006], [0.211, 0.160], [0.109, 0.359], [0.045, 0.574], [0.007, 0.794], [0.037, 0.997], [0.241, 0.920], [0.376, 0.741], [0.404, 0.524], [0.403, 0.473], [0.513, 0.668], [0.646, 0.848]];
const V_LET_A_0: GlyphVariant = GlyphVariant { advance: 0.646, strokes: &[S_LET_A_0_0] };
const S_LET_A_1_0: &[[f32; 2]] = &[[0.704, 0.000], [0.546, 0.048], [0.394, 0.118], [0.250, 0.202], [0.118, 0.304], [0.006, 0.426], [0.118, 0.488], [0.269, 0.419], [0.410, 0.329], [0.502, 0.354], [0.514, 0.521], [0.514, 0.688], [0.506, 0.854], [0.509, 0.996]];
const V_LET_A_1: GlyphVariant = GlyphVariant { advance: 0.704, strokes: &[S_LET_A_1_0] };
const GLYPH_LET_A: &[GlyphVariant] = &[V_LET_A_0, V_LET_A_1];
const S_LET_B_0_0: &[[f32; 2]] = &[[0.099, 0.000], [0.099, 0.253], [0.093, 0.505], [0.075, 0.757], [0.064, 0.943], [0.001, 0.702], [0.034, 0.452], [0.192, 0.265], [0.434, 0.219], [0.590, 0.397], [0.606, 0.649], [0.473, 0.849], [0.249, 0.964], [0.007, 0.983]];
const V_LET_B_0: GlyphVariant = GlyphVariant { advance: 0.606, strokes: &[S_LET_B_0_0] };
const S_LET_B_1_0: &[[f32; 2]] = &[[0.000, 0.000], [0.065, 0.140], [0.118, 0.285], [0.166, 0.432], [0.206, 0.581], [0.226, 0.734], [0.219, 0.717], [0.249, 0.567], [0.374, 0.509], [0.489, 0.608], [0.569, 0.740], [0.582, 0.888], [0.458, 0.973], [0.307, 1.000]];
const V_LET_B_1: GlyphVariant = GlyphVariant { advance: 0.592, strokes: &[S_LET_B_1_0] };
const GLYPH_LET_B: &[GlyphVariant] = &[V_LET_B_0, V_LET_B_1];
const S_LET_C_0_0: &[[f32; 2]] = &[[0.561, 0.018], [0.425, 0.006], [0.292, 0.039], [0.182, 0.121], [0.107, 0.237], [0.049, 0.362], [0.035, 0.497], [0.005, 0.629], [0.033, 0.764], [0.109, 0.879], [0.220, 0.958], [0.355, 0.987], [0.490, 0.985], [0.623, 0.947]];
const V_LET_C_0: GlyphVariant = GlyphVariant { advance: 0.623, strokes: &[S_LET_C_0_0] };
const S_LET_C_1_0: &[[f32; 2]] = &[[0.641, 0.000], [0.511, 0.035], [0.387, 0.088], [0.267, 0.148], [0.161, 0.231], [0.068, 0.329], [0.023, 0.456], [0.000, 0.588], [0.039, 0.714], [0.119, 0.821], [0.225, 0.905], [0.350, 0.954], [0.481, 0.984], [0.615, 1.000]];
const V_LET_C_1: GlyphVariant = GlyphVariant { advance: 0.641, strokes: &[S_LET_C_1_0] };
const GLYPH_LET_C: &[GlyphVariant] = &[V_LET_C_0, V_LET_C_1];
const S_LET_T_0_0: &[[f32; 2]] = &[[0.337, 0.000], [0.293, 0.086], [0.263, 0.178], [0.245, 0.272], [0.234, 0.368], [0.229, 0.464], [0.231, 0.561], [0.231, 0.657], [0.241, 0.753], [0.257, 0.848], [0.295, 0.936], [0.371, 0.989], [0.464, 0.986], [0.558, 0.962]];
const S_LET_T_0_1: &[[f32; 2]] = &[[0.000, 0.337], [0.048, 0.337], [0.097, 0.337], [0.145, 0.332], [0.193, 0.327], [0.241, 0.327], [0.290, 0.324], [0.338, 0.319], [0.386, 0.315], [0.434, 0.310], [0.482, 0.304], [0.530, 0.296], [0.577, 0.287], [0.625, 0.279]];
const V_LET_T_0: GlyphVariant = GlyphVariant { advance: 0.625, strokes: &[S_LET_T_0_0, S_LET_T_0_1] };
const S_LET_T_1_0: &[[f32; 2]] = &[[0.362, 0.000], [0.316, 0.083], [0.280, 0.170], [0.252, 0.261], [0.233, 0.354], [0.224, 0.448], [0.224, 0.543], [0.235, 0.636], [0.265, 0.726], [0.314, 0.807], [0.377, 0.877], [0.452, 0.933], [0.539, 0.970], [0.629, 1.000]];
const S_LET_T_1_1: &[[f32; 2]] = &[[0.000, 0.552], [0.020, 0.552], [0.040, 0.552], [0.060, 0.552], [0.080, 0.552], [0.099, 0.552], [0.119, 0.552], [0.139, 0.552], [0.159, 0.552], [0.179, 0.552], [0.199, 0.552], [0.219, 0.552], [0.239, 0.552], [0.259, 0.552]];
const V_LET_T_1: GlyphVariant = GlyphVariant { advance: 0.629, strokes: &[S_LET_T_1_0, S_LET_T_1_1] };
const GLYPH_LET_T: &[GlyphVariant] = &[V_LET_T_0, V_LET_T_1];
const S_LET_N_0_0: &[[f32; 2]] = &[[0.059, 0.093], [0.036, 0.331], [0.018, 0.570], [0.008, 0.809], [0.015, 0.955], [0.081, 0.725], [0.156, 0.498], [0.241, 0.274], [0.358, 0.067], [0.551, 0.050], [0.617, 0.278], [0.635, 0.517], [0.636, 0.756], [0.602, 0.992]];
const V_LET_N_0: GlyphVariant = GlyphVariant { advance: 0.636, strokes: &[S_LET_N_0_0] };
const S_LET_N_1_0: &[[f32; 2]] = &[[0.000, 0.199], [0.000, 0.389], [0.000, 0.579], [0.000, 0.770], [0.000, 0.960], [0.052, 0.860], [0.104, 0.677], [0.167, 0.498], [0.247, 0.325], [0.353, 0.167], [0.476, 0.026], [0.583, 0.121], [0.623, 0.307], [0.665, 0.492]];
const V_LET_N_1: GlyphVariant = GlyphVariant { advance: 0.665, strokes: &[S_LET_N_1_0] };
const GLYPH_LET_N: &[GlyphVariant] = &[V_LET_N_0, V_LET_N_1];
const S_LET_K_0_0: &[[f32; 2]] = &[[0.000, 0.000], [0.000, 0.077], [0.009, 0.153], [0.016, 0.230], [0.016, 0.307], [0.016, 0.384], [0.016, 0.461], [0.016, 0.538], [0.016, 0.615], [0.016, 0.692], [0.016, 0.769], [0.016, 0.846], [0.016, 0.923], [0.016, 1.000]];
const S_LET_K_0_1: &[[f32; 2]] = &[[0.632, 0.205], [0.563, 0.261], [0.495, 0.318], [0.425, 0.372], [0.354, 0.425], [0.282, 0.476], [0.211, 0.528], [0.156, 0.597], [0.144, 0.681], [0.179, 0.762], [0.234, 0.830], [0.303, 0.886], [0.378, 0.931], [0.459, 0.967]];
const V_LET_K_0: GlyphVariant = GlyphVariant { advance: 0.632, strokes: &[S_LET_K_0_0, S_LET_K_0_1] };
const S_LET_K_1_0: &[[f32; 2]] = &[[0.165, 0.587], [0.196, 0.556], [0.226, 0.526], [0.257, 0.495], [0.288, 0.466], [0.321, 0.437], [0.353, 0.409], [0.388, 0.383], [0.422, 0.357], [0.458, 0.332], [0.494, 0.309], [0.531, 0.288], [0.567, 0.264], [0.603, 0.240]];
const S_LET_K_1_1: &[[f32; 2]] = &[[0.000, 0.463], [0.048, 0.503], [0.096, 0.542], [0.143, 0.583], [0.190, 0.623], [0.237, 0.664], [0.284, 0.704], [0.331, 0.745], [0.376, 0.789], [0.421, 0.831], [0.467, 0.873], [0.512, 0.916], [0.559, 0.956], [0.603, 1.000]];
const S_LET_K_1_2: &[[f32; 2]] = &[[0.074, 0.000], [0.084, 0.074], [0.092, 0.148], [0.101, 0.222], [0.114, 0.296], [0.127, 0.369], [0.140, 0.443], [0.154, 0.516], [0.167, 0.589], [0.181, 0.663], [0.189, 0.737], [0.198, 0.811], [0.210, 0.885], [0.215, 0.959]];
const V_LET_K_1: GlyphVariant = GlyphVariant { advance: 0.603, strokes: &[S_LET_K_1_0, S_LET_K_1_1, S_LET_K_1_2] };
const GLYPH_LET_K: &[GlyphVariant] = &[V_LET_K_0, V_LET_K_1];

fn lookup_glyph(ch: char) -> Option<&'static [GlyphVariant]> {
    match ch {
        '0' => Some(GLYPH_ZERO),
        '1' => Some(GLYPH_ONE),
        '2' => Some(GLYPH_TWO),
        '3' => Some(GLYPH_THREE),
        '4' => Some(GLYPH_FOUR),
        '5' => Some(GLYPH_FIVE),
        '6' => Some(GLYPH_SIX),
        '7' => Some(GLYPH_SEVEN),
        '8' => Some(GLYPH_EIGHT),
        '9' => Some(GLYPH_NINE),
        '.' => Some(GLYPH_DOT),
        '-' => Some(GLYPH_MINUS),
        '+' => Some(GLYPH_PLUS),
        'e' | 'E' => Some(GLYPH_E),
        ',' => Some(GLYPH_COMMA),
        'x' => Some(GLYPH_LET_X),
        'y' => Some(GLYPH_LET_Y),
        'z' => Some(GLYPH_LET_Z),
        'a' => Some(GLYPH_LET_A),
        'b' => Some(GLYPH_LET_B),
        'c' => Some(GLYPH_LET_C),
        't' => Some(GLYPH_LET_T),
        'n' => Some(GLYPH_LET_N),
        'k' => Some(GLYPH_LET_K),
        _ => None,
    }
}

/// Simple deterministic hash in `[0.0, 1.0)` for organic per-glyph handwriting variation.
fn hash_unit(seed: u64, salt: u64) -> f32 {
    let mut z = seed.wrapping_add(salt.wrapping_mul(0x9e37_79b9_7f4a_7c15));
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^= z >> 31;
    ((z >> 40) as f32) / ((1u32 << 24) as f32)
}

/// Evaluate a smooth Catmull-Rom point between `p1` and `p2` at `t in [0, 1]`.
fn catmull_rom(p0: [f32; 2], p1: [f32; 2], p2: [f32; 2], p3: [f32; 2], t: f32) -> [f32; 2] {
    let t2 = t * t;
    let t3 = t2 * t;
    let interp = |a: f32, b: f32, c: f32, d: f32| {
        0.5 * ((2.0 * b)
            + (-a + c) * t
            + (2.0 * a - 5.0 * b + 4.0 * c - d) * t2
            + (-a + 3.0 * b - 3.0 * c + d) * t3)
    };
    [
        interp(p0[0], p1[0], p2[0], p3[0]),
        interp(p0[1], p1[1], p2[1], p3[1]),
    ]
}

fn densify_spline(pts: &[[f32; 2]], subdivisions: usize) -> Vec<[f32; 2]> {
    if pts.len() < 2 {
        return pts.to_vec();
    }
    let n = pts.len();
    let mut out = Vec::with_capacity((n - 1) * subdivisions + 1);
    for i in 0..(n - 1) {
        let p0 = if i == 0 { pts[0] } else { pts[i - 1] };
        let p1 = pts[i];
        let p2 = pts[i + 1];
        let p3 = if i + 2 < n { pts[i + 2] } else { pts[n - 1] };
        for step in 0..subdivisions {
            let t = step as f32 / subdivisions as f32;
            out.push(catmull_rom(p0, p1, p2, p3, t));
        }
    }
    out.push(pts[n - 1]);
    out
}

/// Synthesize handwritten `InkStroke`s for `answer` starting at `start_x`, aligned to `style`,
/// together with natural sequential write-on animation timings.
/// If `page_bounds` is `Some((w, h))`, scales/clamps gently so the answer stays inside the page.
pub fn synthesize_handwritten_answer(
    answer: &str,
    start_x: f32,
    style: &HandwritingStyle,
    seed: u64,
    page_bounds: Option<(f32, f32)>,
) -> SynthesizedAnswer {
    let base_h = style.glyph_height.clamp(14.0, 140.0);

    // Measure total natural width first so we can gently shrink only if close to a fixed page edge.
    let mut natural_w = 0.0_f32;
    let mut chosen: Vec<(char, &'static GlyphVariant, f32, f32)> = Vec::with_capacity(answer.len());
    let mut next_is_superscript = false;
    for (idx, ch) in answer.chars().enumerate() {
        if ch == '^' {
            next_is_superscript = true;
            continue;
        }
        let Some(variants) = lookup_glyph(ch) else {
            next_is_superscript = false;
            continue;
        };
        let pick = ((seed.wrapping_add((idx as u64).wrapping_mul(17))) as usize) % variants.len();
        let var = &variants[pick];
        let (h_mul, y_offset) = if next_is_superscript {
            next_is_superscript = false;
            (0.54, -0.18)
        } else {
            match ch {
                'e' | 'E' | 'x' | 'z' | 'a' | 'c' | 'n' => (0.68, 0.30),
                'y' => (0.86, 0.30),
                'b' | 't' | 'k' => (0.92, 0.06),
                '+' => (0.68, 0.16),
                ',' => (0.34, 0.78),
                _ => (1.0, 0.0),
            }
        };
        natural_w += var.advance * base_h * h_mul + base_h * 0.17;
        chosen.push((ch, var, h_mul, y_offset));
    }
    if chosen.is_empty() {
        return SynthesizedAnswer {
            strokes: Vec::new(),
            reveals: Vec::new(),
            write_end_ms: 0,
        };
    }

    let fit_scale = if let Some((pw, _)) = page_bounds {
        let avail = (pw - 12.0 - start_x).max(base_h * 1.2);
        if natural_w > avail {
            (avail / natural_w).clamp(0.55, 1.0)
        } else {
            1.0
        }
    } else {
        1.0
    };
    let h = base_h * fit_scale;
    let top_y = style.midline_y - h * 0.50;

    // Match user's pen style with slightly reduced stabilization so our smooth spline stays crisp.
    let mut pen = style.pen.clone();
    pen.stabilization = pen.stabilization.min(0.18);
    let base_pressure = style.pressure.clamp(0.30, 0.88);
    let base_slant = style.slant.clamp(-0.22, 0.28);

    let mut strokes_out = Vec::new();
    let mut raw_timings: Vec<(Id, f32, f32)> = Vec::new();
    let mut x_cursor = start_x;
    let mut time_ms: u64 = 1_000;
    const CHAR_STAGGER_MS: f32 = 65.0;

    for (idx, (ch, var, h_mul, y_offset)) in chosen.into_iter().enumerate() {
        // Stagger each character slightly after the previous character *starts*,
        // so consecutive numbers flow in a natural overlapping cascade.
        let mut stroke_cursor_ms = (idx as f32) * CHAR_STAGGER_MS;
        let salt = (idx as u64 + 1).wrapping_mul(31);
        let scale_jit = 1.0 + (hash_unit(seed, salt) - 0.5) * 0.06;
        let y_jit = (hash_unit(seed, salt + 1) - 0.5) * 0.045 * h;
        let slant_jit = base_slant + (hash_unit(seed, salt + 2) - 0.5) * 0.05;
        let wave_phase = hash_unit(seed, salt + 3) * std::f32::consts::TAU;
        let gh = h * h_mul * scale_jit;
        let gy = top_y + y_offset * h + y_jit;

        for (si, raw_stroke) in var.strokes.iter().enumerate() {
            let subdiv = if ch == '.' { 2 } else { 3 };
            let dense = densify_spline(raw_stroke, subdiv);
            if dense.is_empty() {
                continue;
            }
            let mut builder = StrokeBuilder::new(pen.clone());
            let total = dense.len();
            let mut prev_pos: Option<Point> = None;
            let mut arc_len = 0.0_f32;

            for (pi, &[nx, ny]) in dense.iter().enumerate() {
                let frac = if total > 1 {
                    pi as f32 / (total - 1) as f32
                } else {
                    0.5
                };
                // Subtle natural hand tremor (smooth low-frequency wave, zero jaggedness).
                let wobble_x = (frac * std::f32::consts::TAU + wave_phase + si as f32).sin() * 0.008 * gh;
                let wobble_y = (frac * std::f32::consts::PI * 1.5 + wave_phase).cos() * 0.006 * gh;

                // Apply slant around vertical midpoint (ny = 0.5).
                let slanted_x = nx + (0.5 - ny) * slant_jit;
                let mut px = x_cursor + slanted_x * gh + wobble_x;
                let mut py = gy + ny * gh + wobble_y;
                if let Some((pw, ph)) = page_bounds {
                    px = px.clamp(2.0, (pw - 2.0).max(2.0));
                    py = py.clamp(2.0, (ph - 2.0).max(2.0));
                }
                let pos = Point::new(px, py);

                // Realistic handwriting pressure envelope: gentle pen-down, fuller on downstrokes, tapered lift-off.
                let edge_env = (frac * 5.0).min(1.0) * ((1.0 - frac) * 5.0).min(1.0);
                let dy_bias = prev_pos
                    .map(|prev| {
                        let dy = pos.y - prev.y;
                        let dx = pos.x - prev.x;
                        let len = dx.hypot(dy);
                        arc_len += len;
                        (dy / len.max(1e-3)) * 0.08
                    })
                    .unwrap_or(0.0);
                let pressure = (base_pressure * (0.62 + 0.38 * edge_env) + dy_bias).clamp(0.18, 0.96);

                time_ms += 6;
                builder.push(StrokePoint::new(pos, pressure, time_ms));
                prev_pos = Some(pos);
            }

            if let Some(stroke) = builder.finish() {
                let norm_len = (arc_len / h.max(1.0)).clamp(0.0, 5.0);
                let dur = if ch == '.' {
                    75.0
                } else {
                    (115.0 + norm_len * 52.0).clamp(115.0, 245.0)
                };
                raw_timings.push((stroke.id, stroke_cursor_ms, dur));
                // Within a multi-stroke glyph (e.g. 4, 5, +), start the second stroke near the end of the first.
                stroke_cursor_ms += dur * 0.72;
                strokes_out.push(stroke);
            }
            time_ms += 24;
        }

        let spacing = match ch {
            '.' => 0.13,
            '-' => 0.14,
            _ => 0.17,
        };
        let kern_jit = (hash_unit(seed, salt + 4) - 0.5) * 0.03;
        x_cursor += var.advance * gh + h * (spacing + kern_jit);
    }

    let max_end_ms = raw_timings
        .iter()
        .map(|(_, s, d)| s + d)
        .fold(0.0_f32, f32::max);
    // Keep very long multi-digit answers brisk while preserving the cascade overlap.
    let time_scale = if max_end_ms > 900.0 {
        900.0 / max_end_ms
    } else {
        1.0
    };
    let reveals: Vec<StrokeReveal> = raw_timings
        .into_iter()
        .map(|(id, start, dur)| StrokeReveal {
            id,
            start_ms: (start * time_scale).round() as u32,
            duration_ms: ((dur * time_scale).round() as u32).max(40),
        })
        .collect();
    let write_end_ms = reveals
        .iter()
        .map(|r| r.start_ms.saturating_add(r.duration_ms))
        .max()
        .unwrap_or(0);

    SynthesizedAnswer {
        strokes: strokes_out,
        reveals,
        write_end_ms,
    }
}

