<script lang="ts">
  interface Props {
    text: string;
  }

  let { text }: Props = $props();
</script>

<span
  class="absolute top-full left-1/2 -translate-x-1/2 mt-1.5 px-2 py-0.5 rounded
    bg-slate-800 border border-slate-700 text-slate-200 text-xs whitespace-nowrap
    opacity-0 group-hover:opacity-100 transition-opacity pointer-events-none z-20"
>
  {text}
</span>
