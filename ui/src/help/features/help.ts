import { msg } from "../../i18n";
import type { Feature } from "../features";

/** The help window's own controls (spec.md 3.5). `help:open` opens the window. */
const open = ["help:open"];

const features: Feature[] = [
  { id: "help:search", label: msg("Search help"), description: msg("Find help pages by any word in them."),
    keywords: [msg("find"), msg("manual")], topic: "search", reveal: open },
  { id: "help:topic", label: msg("Help topics"), description: msg("The list of help pages, one per area."),
    keywords: [msg("pages"), msg("contents")], topic: "search", reveal: open },
  { id: "help:related", label: msg("Related pages"), description: msg("Go to a page about a related area."),
    keywords: [msg("see also"), msg("links")], topic: "search", reveal: open },
  { id: "help:close", label: msg("Close help"), description: msg("Close the help window."),
    keywords: [msg("done"), msg("exit")], topic: "search", reveal: open },
];

export default features;
