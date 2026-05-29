export default function ReviewPanel({ reviewId }: { reviewId?: unknown }) {
  if (!reviewId) return null;
  return (
    <div className="card space-y-1">
      <div className="text-sm font-semibold">Execution Review</div>
      <div className="text-xs" style={{color:"var(--text-muted)"}}>review_id: <span className="font-mono">{String(reviewId)}</span></div>
    </div>
  );
}
