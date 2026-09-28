import { msg } from "../../i18n";
import type { Feature } from "../features";

/**
 * The 3D polar stage's controls (spec.md 10.1–10.3). They are on screen only
 * while the 3D stage is, so each reveals it first.
 */
const onStage = ["stage:3d"];
const topic = "polar-3d";

const features: Feature[] = [
  { id: "view3d:layout", label: msg("3D layout"), description: msg("Draw the polar as a tower (angle, radius, height) or on straight axes."),
    keywords: [msg("polar tower"), msg("Cartesian"), msg("axes")], topic, reveal: onStage },
  { id: "view3d:camera-top", label: msg("Top view"), description: msg("Look down the wind-speed axis: the classic polar diagram."),
    keywords: [msg("camera"), msg("preset"), msg("from above")], topic, reveal: onStage },
  { id: "view3d:camera-side", label: msg("Side view"), description: msg("Look across the wind-speed axis, each wind speed a level."),
    keywords: [msg("camera"), msg("preset"), "TWS"], topic, reveal: onStage },
  { id: "view3d:camera-iso", label: msg("Isometric view"), description: msg("The three-quarter view of the whole scene."),
    keywords: [msg("camera"), msg("preset"), msg("reset view")], topic, reveal: onStage },
  { id: "view3d:tool-rotate", label: msg("Rotate tool"), description: msg("Drag to turn the 3D view; click a dot to select it."),
    keywords: [msg("orbit"), msg("pan"), msg("zoom")], topic, reveal: onStage },
  { id: "view3d:tool-lasso", label: msg("Lasso selection"), description: msg("Draw round dots in the 3D view to select them."),
    keywords: [msg("select"), msg("freehand")], topic, reveal: onStage },
  { id: "view3d:tool-box", label: msg("Box selection"), description: msg("Drag a rectangle in the 3D view to select the dots inside."),
    keywords: [msg("select"), msg("rectangle")], topic, reveal: onStage },
  { id: "view3d:show-samples", label: msg("Show samples"), description: msg("Show every track sample as a dot."),
    keywords: [msg("dots"), msg("tracks")], topic, reveal: onStage },
  { id: "view3d:show-nodes", label: msg("Show polar nodes"), description: msg("Show the grid points of every visible polar source."),
    keywords: [msg("dots"), msg("grid points")], topic, reveal: onStage },
  { id: "view3d:show-surfaces", label: msg("Show surfaces"), description: msg("Show each visible polar source as a translucent surface."),
    keywords: [msg("mesh"), msg("polar")], topic, reveal: onStage },
  { id: "view3d:show-filtered", label: msg("Show filtered samples"), description: msg("Show the samples the filters remove, dimmed."),
    keywords: [msg("filters"), msg("dimmed")], topic, reveal: onStage },
  { id: "view3d:colour", label: msg("Colour dots by"), description: msg("Colour the dots by source, wave height, current speed or time."),
    keywords: [msg("colour"), "Hs", msg("current"), msg("time")], topic, reveal: onStage },
  { id: "view3d:exclude", label: msg("Exclude from the blend"), description: msg("Remove the selected dots from the blend. Undo puts them back."),
    keywords: [msg("remove"), msg("outlier"), msg("selection")], topic, reveal: onStage },
  { id: "view3d:include", label: msg("Include in the blend"), description: msg("Put excluded dots back into the blend."),
    keywords: [msg("restore"), msg("selection")], topic, reveal: onStage },
  { id: "view3d:show-on-map", label: msg("Show selection on the map"), description: msg("Highlight the selected samples on the map. Arrives with tracks."),
    keywords: [msg("map"), msg("selection")], topic, reveal: onStage },
  { id: "view3d:clear-selection", label: msg("Clear the selection"), description: msg("Select nothing in the 3D view."),
    keywords: [msg("deselect"), "Esc"], topic, reveal: onStage },
];

export default features;
