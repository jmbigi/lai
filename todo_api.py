#!/usr/bin/env python3
"""A FastAPI application for a simple todo API."""

import sys
import os
from typing import List, Optional, Dict, Any
from fastapi import FastAPI, HTTPException, Depends, Query
from pydantic import BaseModel
from typing import Annotated
from fastapi.security import APIKeyHeader
from fastapi.responses import JSONResponse
from fastapi.middleware.cors import CORSMiddleware
from dataclasses import dataclass
from pathlib import Path
import uvicorn


@dataclass
class TodoItem:
    """Data class representing a todo item."""
    id: int
    title: str
    completed: bool


class TodoService:
    """Service class for managing todo items."""
    
    def __init__(self):
        """Initialize the todo service."""
        self.todos: List[TodoItem] = []
        self.next_id: int = 1
    
    def create_todo(self, title: str) -> TodoItem:
        """Create a new todo item.
        
        Args:
            title: The title of the todo item.
            
        Returns:
            The created todo item.
        """
        todo = TodoItem(self.next_id, title, False)
        self.todos.append(todo)
        self.next_id += 1
        return todo
    
    def get_todos(self, completed: Optional[bool] = None) -> List[TodoItem]:
        """Get all todo items with optional filtering by completion status.
        
        Args:
            completed: Filter todos by completion status.
            
        Returns:
            List of todo items matching the filter.
        """
        if completed is None:
            return self.todos
        return [todo for todo in self.todos if todo.completed == completed]
    
    def update_todo(self, todo_id: int, title: Optional[str] = None, completed: Optional[bool] = None) -> TodoItem:
        """Update a todo item.
        
        Args:
            todo_id: The ID of the todo item to update.
            title: New title for the todo item.
            completed: New completion status for the todo item.
            
        Returns:
            The updated todo item.
            
        Raises:
            HTTPException: If the todo item is not found.
        """
        for todo in self.todos:
            if todo.id == todo_id:
                if title is not None:
                    todo.title = title
                if completed is not None:
                    todo.completed = completed
                return todo
        raise HTTPException(status_code=404, detail="Todo item not found")
    
    def delete_todo(self, todo_id: int) -> None:
        """Delete a todo item.
        
        Args:
            todo_id: The ID of the todo item to delete.
            
        Raises:
            HTTPException: If the todo item is not found.
        """
        for todo in self.todos:
            if todo.id == todo_id:
                self.todos.remove(todo)
                return
        raise HTTPException(status_code=404, detail="Todo item not found")


app = FastAPI(title="Todo API", description="A simple todo API with FastAPI")

app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_methods=["*"],
    allow_headers=["*"],
)

todo_service = TodoService()


class TodoCreateRequest(BaseModel):
    """Request model for creating a todo item."""
    title: str


class TodoUpdateRequest(BaseModel):
    """Request model for updating a todo item."""
    title: Optional[str] = None
    completed: Optional[bool] = None


@app.get("/todos", response_model=List[TodoItem])
def get_todos(
    completed: Annotated[Optional[bool], Query(description="Filter todos by completion status")] = None
) -> List[TodoItem]:
    """Get all todo items with optional filtering by completion status."""
    return todo_service.get_todos(completed)


@app.post("/todos", response_model=TodoItem)
def create_todo(todo: TodoCreateRequest) -> TodoItem:
    """Create a new todo item."""
    return todo_service.create_todo(todo.title)


@app.get("/todos/{todo_id}", response_model=TodoItem)
def get_todo(todo_id: int) -> TodoItem:
    """Get a specific todo item by ID."""
    todos = todo_service.get_todos()
    for todo in todos:
        if todo.id == todo_id:
            return todo
    raise HTTPException(status_code=404, detail="Todo item not found")


@app.put("/todos/{todo_id}", response_model=TodoItem)
def update_todo(
    todo_id: int,
    todo: TodoUpdateRequest
) -> TodoItem:
    """Update a specific todo item by ID."""
    return todo_service.update_todo(todo_id, todo.title, todo.completed)


@app.delete("/todos/{todo_id}", response_model=Dict[str, str])
def delete_todo(todo_id: int) -> Dict[str, str]:
    """Delete a specific todo item by ID."""
    todo_service.delete_todo(todo_id)
    return {"message": "Todo item deleted successfully"}


@app.get("/todos/count", response_model=int)
def get_todo_count() -> int:
    """Get the total number of todo items."""
    return len(todo_service.todos)


def main() -> None:
    """Main entry point."""
    try:
        uvicorn.run(app, host="0.0.0.0", port=8000)
    except Exception as e:
        print(f"Error: {e}")
        sys.exit(1)


if __name__ == "__main__":
    main()