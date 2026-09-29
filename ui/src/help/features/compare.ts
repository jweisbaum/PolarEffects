import { msg } from "../../i18n";
import type { Feature } from "../features";

/**
 * The Compare stage's controls (spec.md 11). They are on screen only while
 * the Compare stage is, so each reveals it first.
 */
const onStage = ["stage:compare"];
const topic = "compare";

const features: Feature[] = [
  { id: "compare:operand-a", label: msg("Operand A"), description: msg("Choose what A is: the blend, a source's polar or a track's polar segment."),
    keywords: [msg("compare"), msg("difference"), msg("source"), msg("blend")], topic, reveal: onStage },
  { id: "compare:operand-b", label: msg("Operand B"), description: msg("Choose what B is: the blend, a source's polar or a track's polar segment."),
    keywords: [msg("compare"), msg("difference"), msg("source"), msg("blend")], topic, reveal: onStage },
  { id: "compare:swap", label: msg("Swap A and B"), description: msg("Exchange the two operands; every difference changes sign."),
    keywords: [msg("swap"), msg("exchange"), msg("reverse")], topic, reveal: onStage },
  { id: "compare:show-a", label: msg("Show surface A"), description: msg("Show operand A as a translucent surface in the Compare stage."),
    keywords: [msg("surface"), "3D"], topic, reveal: onStage },
  { id: "compare:show-b", label: msg("Show surface B"), description: msg("Show operand B as a translucent surface in the Compare stage."),
    keywords: [msg("surface"), "3D"], topic, reveal: onStage },
  { id: "compare:show-delta", label: msg("Show the difference surface"), description: msg("Show the surface coloured by A − B between the two operands."),
    keywords: [msg("difference"), msg("delta"), msg("surface")], topic, reveal: onStage },
  { id: "compare:percent", label: msg("Difference as percent"), description: msg("Show each difference as a percentage of B instead of a speed."),
    keywords: [msg("percent"), msg("percentage"), msg("relative")], topic, reveal: onStage },
  { id: "compare:threshold", label: msg("Compare threshold"), description: msg("How much faster a side must be for a cell to count in the regions where A or B is faster."),
    keywords: [msg("threshold"), msg("tolerance"), msg("faster")], topic, reveal: onStage },
  { id: "compare:legend", label: msg("Difference scale"), description: msg("The diverging colour scale, centred on zero, with the range of the differences."),
    keywords: [msg("legend"), msg("colour"), msg("range")], topic, reveal: onStage },
  { id: "compare:summary", label: msg("Compare summary"), description: msg("Cells compared, mean and largest difference, and where each operand is faster."),
    keywords: [msg("statistics"), msg("mean"), msg("maximum"), msg("regions")], topic, reveal: onStage },
  { id: "compare:heat-map", label: msg("Difference heat map"), description: msg("The differences as a flat map, TWA down and TWS across."),
    keywords: [msg("heat map"), "2D", msg("table")], topic, reveal: onStage },
  { id: "compare:layout", label: msg("Compare layout"), description: msg("Draw the comparison as a tower (angle, radius, height) or on straight axes."),
    keywords: [msg("polar tower"), msg("Cartesian"), msg("axes")], topic, reveal: onStage },
  { id: "compare:camera-top", label: msg("Compare from above"), description: msg("Look down the wind-speed axis at the comparison."),
    keywords: [msg("camera"), msg("preset"), msg("from above")], topic, reveal: onStage },
  { id: "compare:camera-side", label: msg("Compare from the side"), description: msg("Look across the wind-speed axis at the comparison."),
    keywords: [msg("camera"), msg("preset"), "TWS"], topic, reveal: onStage },
  { id: "compare:camera-iso", label: msg("Compare in three-quarter view"), description: msg("The three-quarter view of the comparison."),
    keywords: [msg("camera"), msg("preset"), msg("reset view")], topic, reveal: onStage },
];

export default features;
