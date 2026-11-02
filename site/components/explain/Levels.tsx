/**
 * One section of "How OnKey works": the diagrams and the plain explanation, then the exact numbers with where
 * they live in the code. Everyone gets the full depth (an earlier version made readers pick one of three).
 * `hoodTitle`: the heading over the exact numbers, in the page's language.
 */
export function Levels({ id, title, hoodTitle, how, hood }: {
  id: string; title: string; hoodTitle: string;
  sentence?: React.ReactNode; how: React.ReactNode; hood: React.ReactNode;
}) {
  return (
    <section id={id} aria-labelledby={`${id}-title`} className="wide mt-16 scroll-mt-24 first:mt-8">
      <div className="border-b border-line pb-3">
        <h2 id={`${id}-title`} className="!mt-0">{title}</h2>
      </div>
      <div className="pt-6">{how}</div>
      <h3 className="!mt-10 text-base font-semibold text-fg">{hoodTitle}</h3>
      <div className="pt-3">{hood}</div>
    </section>
  );
}
