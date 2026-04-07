"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import { ForumList } from "@/components/forum/ForumList";
import { CreatePostModal } from "@/components/forum/CreatePostModal";

export function CommunityForumClient() {
  const router = useRouter();
  const [showCreatePost, setShowCreatePost] = useState(false);

  const handlePostClick = (postId: string) => {
    router.push(`/community/${postId}`);
  };

  const handleVote = (postId: string, vote: "up" | "down") => {
    // In a real app, this would call an API
    console.log("Vote:", postId, vote);
  };

  const handleCreatePost = (post: {
    title: string;
    category: string;
    content: string;
    tags: string[];
  }) => {
    // In a real app, this would POST to an API
    console.log("Create post:", post);
  };

  return (
    <>
      <ForumList
        posts={[]}
        onPostClick={handlePostClick}
        onVote={handleVote}
        onCreatePost={() => setShowCreatePost(true)}
      />
      <CreatePostModal
        isOpen={showCreatePost}
        onClose={() => setShowCreatePost(false)}
        onSubmit={handleCreatePost}
      />
    </>
  );
}
