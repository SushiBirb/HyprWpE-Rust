pub mod glsl_preprocess;
pub mod scene_generator;

pub use glsl_preprocess::{
    convert_to_es300, parse_combos, parse_parameters, preprocess_program, resolve_includes,
    HLSL_SHIMS,
};
pub use scene_generator::{generate_scene_html, EMBEDDED_SCENE_TEMPLATE};

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::tempdir;

    #[test]
    fn test_glsl_es300_conversions() {
        let input = "float frequency = floor(shapeCoord.x * u_BarCount) / u_BarCount * RESOLUTION;\nuint barFreq1 = frequency % RESOLUTION;\nuint barFreq2 = (barFreq1 + 1) % RESOLUTION;";
        let mut combos = std::collections::HashMap::new();
        combos.insert("RESOLUTION".to_string(), 16);
        let frag_es = convert_to_es300(input, "fragment", &combos);
        assert!(frag_es.contains("float(RESOLUTION)"));
        assert!(frag_es.contains("1u"));
        assert!(frag_es.contains("% uint(RESOLUTION)"));
    }

    #[test]
    fn test_combo_parsing() {
        let shader = r#"
            // [COMBO] {"combo":"MASK", "default":1}
            // [COMBO] {"combo":"BLUR", "default":0}
        "#;
        let combos = parse_combos(shader);
        assert_eq!(combos.get("MASK"), Some(&1));
        assert_eq!(combos.get("BLUR"), Some(&0));
    }

    #[test]
    fn test_scene_html_generation() {
        let dir = tempdir().unwrap();
        let scene_json = r#"{
            "general": {
                "clearcolor": "0.1 0.2 0.3",
                "cameraparallax": true,
                "cameraparallaxamount": 0.08,
                "cameraparallaxmouseinfluence": -0.3,
                "orthogonalprojection": {"width": 1920, "height": 1080}
            },
            "objects": [
                {
                    "name": "Background",
                    "visible": true,
                    "image": "models/bg.json"
                },
                {
                    "name": "Kirby",
                    "visible": true,
                    "scale": "2.12 2.12",
                    "parallaxDepth": "0.8 0.8",
                    "image": "models/kirby.png"
                }
            ]
        }"#;

        fs::write(dir.path().join("scene.json"), scene_json).unwrap();
        let success = generate_scene_html(dir.path());
        assert!(success);

        let index_html = dir.path().join("index.html");
        assert!(index_html.exists());
        let html_content = fs::read_to_string(index_html).unwrap();
        assert!(html_content.contains("HyprWpE Scene Player"));
        assert!(html_content.contains("Background"));
        assert!(html_content.contains("isFirstBg"));
        assert!(html_content.contains("parallaxDepth"));
        assert!(html_content.contains("cameraparallaxamount"));
    }

    #[test]
    fn test_sub_sprite_size_resolution_regression() {
        let dir = tempdir().unwrap();
        let scene_json = r#"{
            "general": {
                "orthogonalprojection": {"width": 1920, "height": 1080}
            },
            "objects": [
                {
                    "name": "Pool",
                    "visible": true,
                    "size": "1920 1080",
                    "image": "models/pool.png"
                },
                {
                    "name": "KirbyFloatie",
                    "visible": true,
                    "scale": "2.12 2.12",
                    "image": "models/kirby.png"
                }
            ]
        }"#;

        fs::write(dir.path().join("scene.json"), scene_json).unwrap();
        let success = generate_scene_html(dir.path());
        assert!(success);

        let index_html = dir.path().join("index.html");
        let html_content = fs::read_to_string(index_html).unwrap();
        // Regression test: sub-sprite KirbyFloatie must have size null so getObjectMatrix uses texture dimensions instead of 1920x1080
        assert!(html_content.contains("\"KirbyFloatie\""));
        assert!(html_content.contains("\"size\":null"));
    }

    #[test]
    fn test_material_shader_neonsun_preprocess() {
        let vert_path = PathBuf::from("/tmp/hyprwpe_eval/work_1548688862/shaders/neonsun.vert");
        let frag_path = PathBuf::from("/tmp/hyprwpe_eval/work_1548688862/shaders/neonsun.frag");
        if vert_path.is_file() && frag_path.is_file() {
            let textures = std::collections::HashMap::new();
            let combos = std::collections::HashMap::new();
            let shaders_dir = PathBuf::from("config/shaders");
            let prog = preprocess_program(&vert_path, &frag_path, &textures, Some(&combos), Some(&shaders_dir)).expect("preprocess neonsun");
            assert!(prog.frag_glsl.contains("out_FragColor"));
            assert!(prog.vert_glsl.contains("#version 300 es"));
        }

        let grid_vert = PathBuf::from("/tmp/hyprwpe_eval/work_1548688862/shaders/neongrid.vert");
        let grid_frag = PathBuf::from("/tmp/hyprwpe_eval/work_1548688862/shaders/neongrid.frag");
        if grid_vert.is_file() && grid_frag.is_file() {
            let textures = std::collections::HashMap::new();
            let combos = std::collections::HashMap::new();
            let shaders_dir = PathBuf::from("config/shaders");
            let prog = preprocess_program(&grid_vert, &grid_frag, &textures, Some(&combos), Some(&shaders_dir)).expect("preprocess neongrid");
            assert!(prog.frag_glsl.contains("out_FragColor"));
            assert!(prog.vert_glsl.contains("#version 300 es"));
        }

        let clouds_vert = PathBuf::from("/tmp/hyprwpe_eval/work_1548688862/shaders/cloudsbg.vert");
        let clouds_frag = PathBuf::from("/tmp/hyprwpe_eval/work_1548688862/shaders/cloudsbg.frag");
        if clouds_vert.is_file() && clouds_frag.is_file() {
            let textures = std::collections::HashMap::new();
            let combos = std::collections::HashMap::new();
            let shaders_dir = PathBuf::from("config/shaders");
            let prog = preprocess_program(&clouds_vert, &clouds_frag, &textures, Some(&combos), Some(&shaders_dir)).expect("preprocess cloudsbg");
            assert!(prog.frag_glsl.contains("out_FragColor"));
            assert!(prog.vert_glsl.contains("#version 300 es"));
        }
    }
}
