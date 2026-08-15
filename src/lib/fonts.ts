/** The GB/T 9704 fonts the exporter writes into DOCX files by name. */
export const gbFonts = ["仿宋_GB2312", "楷体_GB2312", "方正小标宋简体"] as const;

export interface FontProbe {
  /** Whether the runtime can probe fonts at all (WebView2 / WebKitGTK can). */
  supported: boolean;
  /** GB/T 9704 fonts not installed on this machine. */
  missing: string[];
}

/**
 * Probes the system for the GB/T 9704 fonts through the FontFaceSet API.
 *
 * The preview uses open-source substitutes while the exported DOCX keeps the standard font
 * names, so missing fonts only matter for on-screen fidelity. `supported` is false in
 * runtimes without `document.fonts` (e.g. jsdom), where the caller should stay quiet.
 */
export function probeGbFonts(): FontProbe {
  const fonts = typeof document !== "undefined" ? document.fonts : undefined;
  if (!fonts || typeof fonts.check !== "function") {
    return { supported: false, missing: [] };
  }
  const missing = gbFonts.filter((font) => !fonts.check(`16px "${font}"`));
  return { supported: true, missing };
}
