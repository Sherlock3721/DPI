/**
 * Svelte akce: připojí MediaStream k <video> až po prvním vykreslení elementu.
 *
 * WebKitGTK (Linux) nespustí přehrávání, pokud `srcObject` dostane element
 * vložený do DOM ve stejném tasku — video zůstane černé (readyState 0, play()
 * nikdy neskončí). Týká se každého <video> vytvořeného nad už běžícím streamem
 * (náhled po zapnutí kamery, maximalizované okno, kalibrační modal).
 */
export function videoStream(node: HTMLVideoElement, stream: MediaStream | null) {
  let frame = 0;

  function attach(s: MediaStream | null) {
    cancelAnimationFrame(frame);
    if (!s) {
      node.srcObject = null;
      return;
    }
    frame = requestAnimationFrame(() => {
      if (node.srcObject !== s) node.srcObject = s;
      node.play().catch(() => {});
    });
  }

  attach(stream);
  return {
    update: attach,
    destroy() {
      cancelAnimationFrame(frame);
      node.srcObject = null;
    },
  };
}
