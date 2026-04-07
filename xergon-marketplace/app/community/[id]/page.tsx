import type { Metadata } from "next";
import { PostPageClient } from "./PostPageClient";

interface PostPageProps {
  params: Promise<{ id: string }>;
}

export async function generateMetadata({ params }: PostPageProps): Promise<Metadata> {
  const { id } = await params;
  return {
    title: `Forum Post | Xergon Network`,
    description: `View post ${id} on the Xergon community forum.`,
  };
}

export default async function PostPage({ params }: PostPageProps) {
  const { id } = await params;
  return <PostPageClient postId={id} />;
}
