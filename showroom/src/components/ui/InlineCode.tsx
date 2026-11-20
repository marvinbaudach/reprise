/**
 * Text read out of a Markdown record keeps its backticks. A reader should see
 * code set as code, not the punctuation that marks it, so every backticked span
 * becomes a `<code>` and everything between them stays plain text.
 */
export function InlineCode({ text }: { readonly text: string }) {
  const parts = text.split('`');
  // An odd number of backticks means an unclosed span; show the text as written
  // rather than guess where the code was meant to end.
  if (parts.length % 2 === 0) return <>{text}</>;
  return (
    <>
      {parts.map((part, index) =>
        index % 2 === 1 ? (
          // biome-ignore lint/suspicious/noArrayIndexKey: the split order is the text order and never changes.
          <code key={index}>{part}</code>
        ) : (
          part
        ),
      )}
    </>
  );
}
