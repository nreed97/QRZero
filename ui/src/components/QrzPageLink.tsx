/** The station's public page on QRZ.com. */
export const qrzPageUrl = (call: string) => `https://www.qrz.com/db/${encodeURI(call.trim().toUpperCase())}`;

/**
 * A plain link styled as a button. In the desktop app the window handler sends it to the
 * system browser; in a browser it just opens a new tab.
 */
export default function QrzPageLink({ call }: { call: string }) {
  const c = call.trim();
  return (
    <a
      className={`button-link${c.length < 3 ? " disabled" : ""}`}
      href={c.length < 3 ? undefined : qrzPageUrl(c)}
      target="_blank"
      rel="noopener noreferrer"
      aria-disabled={c.length < 3}
      title="Open this station's page on QRZ.com in your web browser"
    >View on QRZ.com</a>
  );
}
