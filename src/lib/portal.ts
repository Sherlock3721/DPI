/**
 * Svelte akce: přesune prvek do jiného místa v DOM (podle id cíle), stav
 * a události zůstávají v komponentě, která ho vykresluje. Bez cíle zůstane
 * prvek na svém místě.
 */
export function portal(node: HTMLElement, targetId: string) {
  const target = document.getElementById(targetId);
  if (target) target.appendChild(node);
  return {
    destroy() {
      if (target) node.remove();
    },
  };
}
