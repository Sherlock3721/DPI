<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { submit_feedback } from "../lib/tauri";

  export let show = false;

  let feedbackMessage = "";
  let isSendingFeedback = false;

  const dispatch = createEventDispatcher();

  async function handleFeedbackSubmit() {
    if (!feedbackMessage.trim()) return;

    isSendingFeedback = true;
    try {
      // 1. Uložíme zpětnou vazbu lokálně
      await submit_feedback({
        type: "App Feedback",
        message: feedbackMessage,
        timestamp: new Date().toISOString(),
      });

      // 2. Odeslání zprávy administrátorovi na Matrix
      const MATRIX_SERVER = "https://matrix.7wave.cz";
      const ROOM_ID = "!OPln_hRT-9VZf4fCutS5wpsvdP_W5agt1rYSC31etis";
      const ACCESS_TOKEN = "KdJIOdCGfr1pg8XHIVgVXWicrZJt0FQN";

      const safeRoomId = encodeURIComponent(ROOM_ID);
      const txnId = Date.now().toString();
      const url = `${MATRIX_SERVER}/_matrix/client/v3/rooms/${safeRoomId}/send/m.room.message/${txnId}`;

      const response = await fetch(url, {
        method: "PUT",
        headers: {
          Authorization: `Bearer ${ACCESS_TOKEN}`,
          "Content-Type": "application/json",
        },
        body: JSON.stringify({
          msgtype: "m.text",
          body: `[Zpětná vazba z aplikace]\n${feedbackMessage}`,
        }),
      });

      if (!response.ok) {
        const errorText = await response.text();
        throw new Error(`Matrix HTTP ${response.status}: ${errorText}`);
      }

      alert("Zpětná vazba byla úspěšně odeslána. Děkujeme!");
      show = false;
      feedbackMessage = "";
    } catch (e) {
      console.error(e);
      alert("Nepodařilo se odeslat zpětnou vazbu.");
    } finally {
      isSendingFeedback = false;
    }
  }

  function close() {
    show = false;
  }
</script>

{#if show}
  <div class="fixed inset-0 bg-black/60 backdrop-blur-sm flex items-center justify-center z-[100]">
    <div class="bg-slate-800 border border-slate-700 p-6 rounded-xl shadow-2xl w-full max-w-md">
      <h2 class="text-xl font-bold text-slate-100 mb-4">Nahlásit chybu / Nápad</h2>
      <textarea
        bind:value={feedbackMessage}
        rows="4"
        class="w-full bg-slate-900 border border-slate-700 rounded-lg p-3 text-slate-300 focus:outline-none focus:border-labaccent resize-none mb-4"
        placeholder="Popište co nejpřesněji, s čím potřebujete pomoci nebo co nefunguje..."
      ></textarea>
      <div class="flex justify-end gap-3">
        <button
          on:click={close}
          class="px-4 py-2 rounded-lg text-slate-400 hover:text-slate-200 transition-colors"
        >
          Zrušit
        </button>
        <button
          on:click={handleFeedbackSubmit}
          disabled={isSendingFeedback || !feedbackMessage.trim()}
          class="px-4 py-2 rounded-lg bg-labaccent hover:bg-blue-600 text-white font-medium transition-colors disabled:opacity-50"
        >
          {isSendingFeedback ? "Odesílám..." : "Odeslat"}
        </button>
      </div>
    </div>
  </div>
{/if}
